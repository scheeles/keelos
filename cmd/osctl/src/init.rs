use anyhow::{anyhow, Result};
use keel_api::node::node_service_client::NodeServiceClient;
use keel_api::node::{SignBootstrapCertificateRequest, SignBootstrapCertificateResponse};
use openssl::hash::MessageDigest;
use openssl::pkey::{PKey, Private};
use openssl::rsa::Rsa;
use openssl::x509::{X509Name, X509Req, X509ReqBuilder};
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;

/// Initialize osctl certificates - bootstrap mode
pub async fn init_bootstrap(
    node_addr: &str,
    cert_dir: PathBuf,
) -> Result<()> {
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
        // TODO: Implement CSR approval
        // This requires the user to have appropriate RBAC permissions
        println!("⚠️  Auto-approval not yet implemented");
        println!("   Please approve manually: kubectl certificate approve {}", csr_name);
    } else {
        println!("⚠️  CSR requires manual approval. Run:");
        println!("   kubectl certificate approve {}", csr_name);
    }

    println!("\n⏳ Waiting for certificate to be issued...");
    // TODO: Implement wait loop for certificate
    // For now, just indicate what needs to happen

    println!("\n⚠️  K8s CSR workflow partially implemented");
    println!("   Next steps:");
    println!("   1. Approve the CSR: kubectl certificate approve {}", csr_name);
    println!("   2. Certificate will be available in CSR status");
    println!("   3. Save to {}/client.pem", cert_dir.display());

    Ok(())
}

// Helper function for base64 encoding
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

