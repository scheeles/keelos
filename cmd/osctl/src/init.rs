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

    // TODO: Implement K8s CSR workflow
    // 1. Generate RSA key pair
    // 2. Create CSR
    // 3. Submit to K8s API
    // 4. Wait for approval (or auto-approve)
    // 5. Get signed certificate
    // 6. Save to ~/.keel/

    println!("⚠️  Kubernetes PKI mode not yet implemented");
    println!("Please use bootstrap mode for now");

    Ok(())
}
