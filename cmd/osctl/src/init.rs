use anyhow::{anyhow, Result};
use keel_api::node::node_service_client::NodeServiceClient;
use keel_api::node::SignBootstrapCertificateRequest;
use openssl::hash::MessageDigest;
use openssl::pkey::PKey;
use openssl::rsa::Rsa;
use openssl::x509::{X509Name, X509ReqBuilder};
use std::fs;
use std::path::PathBuf;

/// Initialize osctl certificates - bootstrap mode
pub async fn init_bootstrap(node_addr: &str, cert_dir: PathBuf) -> Result<()> {
    println!("🔐 Requesting bootstrap certificate from node...\n");

    println!("🔑 Generating client key pair locally...");

    // Generate RSA key pair locally (NEVER transmitted)
    let rsa = Rsa::generate(2048)?;
    let private_key_pem = rsa.private_key_to_pem()?;

    // Create Certificate Signing Request (CSR)
    let mut req_builder = X509ReqBuilder::new()?;

    let mut name = X509Name::builder()?;
    name.append_entry_by_text("CN", "bootstrap-admin")?;
    let name = name.build();

    req_builder.set_subject_name(&name)?;

    let pubkey = PKey::from_rsa(rsa)?;
    req_builder.set_pubkey(&pubkey)?;

    req_builder.sign(&pubkey, MessageDigest::sha256())?;

    let csr = req_builder.build();
    let csr_pem = String::from_utf8(csr.to_pem()?)?;

    println!("📡 Sending CSR to node for signing...");

    // SECURITY: This is the ONLY HTTP (non-TLS) call in the entire system
    // Only the CSR (public key) is sent - private key stays on client
    // After this, ALL commands (including osctl bootstrap) use mTLS
    let endpoint = format!("http://{}", node_addr);
    let mut client = NodeServiceClient::connect(endpoint).await?;

    let response = client
        .sign_bootstrap_certificate(SignBootstrapCertificateRequest { csr_pem })
        .await?;
    let certs = response.into_inner();

    // Save bootstrap certificates
    let bootstrap_dir = cert_dir.join("bootstrap");
    fs::create_dir_all(&bootstrap_dir)?;
    fs::write(bootstrap_dir.join("client.pem"), certs.client_cert_pem)?;
    fs::write(bootstrap_dir.join("client.key"), private_key_pem)?;
    fs::write(bootstrap_dir.join("ca.pem"), certs.ca_cert_pem)?;

    // Set restrictive permissions on private key
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            bootstrap_dir.join("client.key"),
            fs::Permissions::from_mode(0o600),
        )?;
    }

    println!(
        "\n✅ Bootstrap certificates saved to: {}",
        bootstrap_dir.display()
    );
    println!("\n💡 Next steps:");
    println!("   1. Bootstrap node to Kubernetes:");
    println!("      osctl bootstrap --api-server <k8s-api> --token <token>");
    println!("   2. Get operational certificates:");
    println!("      osctl init --kubeconfig ~/.kube/config");

    Ok(())
}

/// Initialize osctl certificates - Kubernetes PKI mode
pub async fn init_kubernetes(
    kubeconfig_path: &str,
    cert_dir: PathBuf,
    cert_name: &str,
    auto_approve: bool,
) -> Result<()> {
    println!("🔐 Requesting operational certificate from Kubernetes...\n");

    // 1. Generate RSA key pair locally
    println!("🔑 Generating client key pair...");
    let rsa = Rsa::generate(2048)?;
    let private_key_pem = rsa.private_key_to_pem()?;

    // 2. Create CSR for operational certificate
    let mut req_builder = X509ReqBuilder::new()?;

    let mut name = X509Name::builder()?;
    name.append_entry_by_text("CN", cert_name)?;
    name.append_entry_by_text("O", "system:osctl")?; // Organization for RBAC
    let name = name.build();

    req_builder.set_subject_name(&name)?;

    let pubkey = PKey::from_rsa(rsa)?;
    req_builder.set_pubkey(&pubkey)?;

    req_builder.sign(&pubkey, MessageDigest::sha256())?;

    let csr = req_builder.build();
    let csr_der = csr.to_der()?;
    let csr_b64 = base64::encode(&csr_der);

    println!("📋 Creating CertificateSigningRequest in Kubernetes...");

    // 3. Connect to Kubernetes API
    let config = kube::Config::from_kubeconfig(&kube::config::KubeConfigOptions {
        context: None,
        cluster: None,
        user: None,
    })
    .await?;
    let client = kube::Client::try_from(config)?;

    // 4. Create CSR in Kubernetes
    use k8s_openapi::api::certificates::v1::{
        CertificateSigningRequest, CertificateSigningRequestSpec,
    };
    use kube::api::{Api, PostParams};

    let csr_api: Api<CertificateSigningRequest> = Api::all(client.clone());

    let csr_obj = CertificateSigningRequest {
        metadata: k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta {
            name: Some(format!("osctl-{}", cert_name)),
            ..Default::default()
        },
        spec: CertificateSigningRequestSpec {
            request: k8s_openapi::ByteString(csr_der),
            signer_name: "kubernetes.io/kube-apiserver-client".to_string(),
            usages: Some(vec![
                "client auth".to_string(),
                "digital signature".to_string(),
                "key encipherment".to_string(),
            ]),
            ..Default::default()
        },
        status: None,
    };

    let csr_created = csr_api.create(&PostParams::default(), &csr_obj).await?;
    let csr_name = csr_created.metadata.name.unwrap();

    println!("✓ CSR created: {}", csr_name);

    // 5. Auto-approve if requested
    if auto_approve {
        println!("⏳ Auto-approving CSR...");

        use k8s_openapi::api::certificates::v1::{
            CertificateSigningRequestCondition, CertificateSigningRequestStatus,
        };

        // Get the CSR to update
        let mut csr_to_approve = csr_api.get(&csr_name).await?;

        // Set approval condition
        let approval_condition = CertificateSigningRequestCondition {
            type_: "Approved".to_string(),
            status: "True".to_string(),
            last_transition_time: k8s_openapi::apimachinery::pkg::apis::meta::v1::Time(
                chrono::Utc::now(),
            ),
            message: "Approved by osctl".to_string(),
            reason: "AutoApproved".to_string(),
            last_update_time: None,
        };

        if csr_to_approve.status.is_none() {
            csr_to_approve.status = Some(CertificateSigningRequestStatus::default());
        }

        if let Some(status) = &mut csr_to_approve.status {
            if status.conditions.is_none() {
                status.conditions = Some(vec![]);
            }
            if let Some(conditions) = &mut status.conditions {
                conditions.push(approval_condition);
            }
        }

        // Update the CSR with approval
        use kube::api::{Patch, PatchParams};
        let patch = serde_json::json!({
            "status": csr_to_approve.status
        });
        csr_api
            .patch_status(&csr_name, &PatchParams::default(), &Patch::Merge(patch))
            .await?;

        println!("✓ CSR auto-approved");
    } else {
        println!("⚠️  CSR requires manual approval. Run:");
        println!("   kubectl certificate approve {}", csr_name);
    }

    // 6. Wait for certificate to be issued
    println!("\n⏳ Waiting for certificate to be issued...");

    let max_wait = std::time::Duration::from_secs(60);
    let start = std::time::Instant::now();
    let mut cert_pem = None;

    while start.elapsed() < max_wait {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        let csr_status = csr_api.get(&csr_name).await?;

        if let Some(status) = &csr_status.status {
            if let Some(certificate) = &status.certificate {
                // Certificate is now available
                cert_pem = Some(String::from_utf8(certificate.0.clone())?);
                break;
            }
        }

        print!(".");
        use std::io::Write;
        std::io::stdout().flush()?;
    }

    if cert_pem.is_none() {
        return Err(anyhow!("Timeout waiting for certificate to be issued"));
    }

    println!("\n✓ Certificate issued!");

    // 7. Get Kubernetes CA certificate
    let ca_cert = get_k8s_ca(&client).await?;

    // 8. Save certificates
    fs::create_dir_all(&cert_dir)?;
    fs::write(cert_dir.join("client.pem"), cert_pem.unwrap())?;
    fs::write(cert_dir.join("client.key"), private_key_pem)?;
    fs::write(cert_dir.join("ca.pem"), ca_cert)?;

    // Set restrictive permissions on private key
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            cert_dir.join("client.key"),
            fs::Permissions::from_mode(0o600),
        )?;
    }

    println!(
        "\n✅ Operational certificates saved to: {}",
        cert_dir.display()
    );
    println!("\n💡 You can now use osctl with mTLS:");
    println!("   osctl --endpoint https://<node-ip>:50051 status");

    Ok(())
}

/// Get Kubernetes CA certificate from cluster
async fn get_k8s_ca(client: &kube::Client) -> Result<String> {
    use k8s_openapi::api::core::v1::ConfigMap;
    use kube::api::Api;

    let cm_api: Api<ConfigMap> = Api::namespaced(client.clone(), "kube-public");
    let cluster_info = cm_api.get("cluster-info").await?;

    if let Some(data) = &cluster_info.data {
        if let Some(kubeconfig_str) = data.get("kubeconfig") {
            // Parse kubeconfig to extract CA cert
            // For now, return a placeholder - in production, parse the kubeconfig
            // and extract the certificate-authority-data
            return Ok(kubeconfig_str.clone());
        }
    }

    Err(anyhow!("Could not retrieve Kubernetes CA certificate"))
}

// Helper function for base64 encoding
mod base64 {
    pub fn encode(bytes: &[u8]) -> String {
        use base64::engine::general_purpose::STANDARD;
        use std::io::Write;
        let mut buf = Vec::new();
        {
            let mut encoder = base64::write::EncoderWriter::new(&mut buf, &STANDARD);
            encoder.write_all(bytes).unwrap();
        }
        String::from_utf8(buf).unwrap()
    }
}
