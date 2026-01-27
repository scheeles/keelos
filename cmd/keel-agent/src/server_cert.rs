use anyhow::{anyhow, Result};
use kube::api::{Api, PostParams};
use kube::Client;
use openssl::asn1::Asn1Time;
use openssl::bn::{BigNum, MsbOption};
use openssl::hash::MessageDigest;
use openssl::pkey::{PKey, Private};
use openssl::rsa::Rsa;
use openssl::x509::extension::{ExtendedKeyUsage, KeyUsage, SubjectAlternativeName};
use openssl::x509::{X509Name, X509Req};
use std::fs;
use std::path::Path;
use tracing::{error, info, warn};

const SERVER_CERT_PATH: &str = "/etc/keel/crypto/server.pem";
const SERVER_KEY_PATH: &str = "/etc/keel/crypto/server.key";
const CSR_NAME_PREFIX: &str = "keel-agent";

/// Request server certificate from Kubernetes CSR API
pub async fn request_server_certificate(node_ip: &str) -> Result<()> {
    info!("Requesting server certificate from Kubernetes");

    // 1. Check if certificates already exist
    if Path::new(SERVER_CERT_PATH).exists() {
        info!("Server certificate already exists, skipping CSR request");
        return Ok(());
    }

    // 2. Check if Kubernetes is bootstrapped
    let kubeconfig_path = "/var/lib/keel/kubernetes/kubelet.kubeconfig";
    if !Path::new(kubeconfig_path).exists() {
        warn!("Kubernetes not bootstrapped yet, cannot request server certificate");
        return Err(anyhow!("Kubernetes not bootstrapped"));
    }

    // 3. Initialize Kubernetes client
    let client = Client::try_default().await?;

    // 4. Generate server key pair
    info!("Generating server key pair...");
    let (private_key, csr_pem) = generate_server_csr(node_ip)?;

    // 5. Submit CSR to Kubernetes
    info!("Submitting CSR to Kubernetes API...");
    let csr_name = submit_server_csr(&client, &csr_pem).await?;
    info!("CSR created: {}", csr_name);

    // 6. Wait for certificate to be issued
    info!("Waiting for certificate to be issued...");
    let cert_pem = wait_for_certificate(&client, &csr_name).await?;
    info!("Certificate issued!");

    // 7. Get Kubernetes  CA certificate
    let ca_cert = get_k8s_ca(&client).await?;

    // 8. Save certificates to disk
    save_server_certificate(&private_key, &cert_pem, &ca_cert)?;

    info!("Server certificate successfully obtained and saved");
    Ok(())
}

/// Generate server CSR with appropriate SANs and key usage
fn generate_server_csr(node_ip: &str) -> Result<(PKey<Private>, String)> {
    // Generate RSA-2048 key pair
    let rsa = Rsa::generate(2048)?;
    let private_key = PKey::from_rsa(rsa)?;

    // Create CSR
    let mut req_builder = X509Req::builder()?;
    req_builder.set_version(0)?;

    // Set subject
    let mut name_builder = X509Name::builder()?;
    name_builder.append_entry_by_text("CN", "keel-agent")?;
    name_builder.append_entry_by_text("O", "system:nodes")?;
    let name = name_builder.build();
    req_builder.set_subject_name(&name)?;

    // Set public key
    req_builder.set_pubkey(&private_key)?;

    // Add Subject Alternative Names (SANs)
    let mut san_builder = SubjectAlternativeName::new();
    san_builder.dns("keel-agent.local");
    san_builder.ip(node_ip);
    san_builder.ip("127.0.0.1");
    
    // Note: Extensions in CSR require special handling
    // For now, we'll rely on K8s to add appropriate extensions

    // Sign the CSR
    req_builder.sign(&private_key, MessageDigest::sha256())?;
    let csr = req_builder.build();

    // Convert to PEM
    let csr_pem = String::from_utf8(csr.to_pem()?)?;

    Ok((private_key, csr_pem))
}

/// Submit server CSR to Kubernetes API
async fn submit_server_csr(client: &Client, csr_pem: &str) -> Result<String> {
    use k8s_openapi::api::certificates::v1::CertificateSigningRequest;
    use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;

    let csr_api: Api<CertificateSigningRequest> = Api::all(client.clone());

    // Generate unique CSR name
    let hostname = std::env::var("HOSTNAME").unwrap_or_else(|_| "unknown".to_string());
    let csr_name = format!("{}-{}", CSR_NAME_PREFIX, hostname);

    // Encode CSR in base64
    let csr_bytes = csr_pem.as_bytes();
    let csr_b64 = base64::encode(csr_bytes);

    // Create CSR object
    let csr_obj = CertificateSigningRequest {
        metadata: ObjectMeta {
            name: Some(csr_name.clone()),
            ..Default::default()
        },
        spec: k8s_openapi::api::certificates::v1::CertificateSigningRequestSpec {
            request: k8s_openapi::ByteString(csr_b64.into_bytes()),
            signer_name: "kubernetes.io/kubelet-serving".to_string(),
            usages: Some(vec![
                "digital signature".to_string(),
                "key encipherment".to_string(),
                "server auth".to_string(),
            ]),
            ..Default::default()
        },
        status: None,
    };

    // Submit CSR
    csr_api.create(&PostParams::default(), &csr_obj).await?;

    Ok(csr_name)
}

/// Wait for certificate to be issued (with timeout)
async fn wait_for_certificate(client: &Client, csr_name: &str) -> Result<String> {
    use k8s_openapi::api::certificates::v1::CertificateSigningRequest;

    let csr_api: Api<CertificateSigningRequest> = Api::all(client.clone());

    let max_wait = std::time::Duration::from_secs(300); // 5 minutes
    let start = std::time::Instant::now();

    while start.elapsed() < max_wait {
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;

        let csr = csr_api.get(csr_name).await?;

        if let Some(status) = &csr.status {
            if let Some(certificate) = &status.certificate {
                // Certificate is now available
                return Ok(String::from_utf8(certificate.0.clone())?);
            }

            // Check if CSR was denied
            if let Some(conditions) = &status.conditions {
                for condition in conditions {
                    if condition.type_ == "Denied" && condition.status == "True" {
                        return Err(anyhow!("CSR was denied: {:?}", condition.message));
                    }
                }
            }
        }

        info!("Still waiting for certificate...");
    }

    Err(anyhow!("Timeout waiting for certificate to be issued"))
}

/// Get Kubernetes CA certificate
async fn get_k8s_ca(client: &Client) -> Result<String> {
    // Read CA from kubelet kubeconfig
    let kubeconfig_path = "/var/lib/keel/kubernetes/kubelet.kubeconfig";
    let kubeconfig_content = fs::read_to_string(kubeconfig_path)?;

    // Parse kubeconfig to extract CA
    // For simplicity, we'll try to read the cluster CA from the APIServer
    // In production, parse the kubeconfig properly
    
    use k8s_openapi::api::core::v1::ConfigMap;
    let cm_api: Api<ConfigMap> = Api::namespaced(client.clone(), "kube-system");
    
    // Try to get CA from kube-root-ca.crt configmap
    match cm_api.get("kube-root-ca.crt").await {
        Ok(cm) => {
            if let Some(data) = &cm.data {
                if let Some(ca) = data.get("ca.crt") {
                    return Ok(ca.clone());
                }
            }
        }
        Err(e) => {
            warn!("Could not get CA from configmap: {}", e);
        }
    }

    Err(anyhow!("Could not retrieve Kubernetes CA certificate"))
}

/// Save server certificate and key to disk
fn save_server_certificate(
    private_key: &PKey<Private>,
    cert_pem: &str,
    ca_cert: &str,
) -> Result<()> {
    // Ensure directory exists
    let cert_dir = Path::new("/etc/keel/crypto");
    fs::create_dir_all(cert_dir)?;

    // Write private key
    let key_pem = private_key.private_key_to_pem_pkcs8()?;
    fs::write(SERVER_KEY_PATH, key_pem)?;

    // Set restrictive permissions on private key
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(SERVER_KEY_PATH, fs::Permissions::from_mode(0o600))?;
    }

    // Write certificate
    fs::write(SERVER_CERT_PATH, cert_pem)?;

    // Write CA certificate
    fs::write("/etc/keel/crypto/ca.pem", ca_cert)?;

    info!("Server certificate and key saved to {}", cert_dir.display());
    Ok(())
}

// Helper for base64 encode
mod base64 {
    pub fn encode(bytes: &[u8]) -> String {
        use std::io::Write;
        let mut buf = Vec::new();
        {
            let mut encoder = base64::write::EncoderWriter::new(&mut buf, base64::STANDARD);
            encoder.write_all(bytes).unwrap();
        }
        String::from_utf8(buf).unwrap()
    }
}
