use rcgen::{Certificate, CertificateParams, DistinguishedName, DnType, KeyPair};
use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing::{error, info, warn};

const BOOTSTRAP_CERT_DIR: &str = "/etc/keel/crypto";
const BOOTSTRAP_CA_CERT: &str = "bootstrap-ca.pem";
const BOOTSTRAP_CA_KEY: &str = "bootstrap-ca-key.pem";
const BOOTSTRAP_CLIENT_CERT: &str = "bootstrap-client.pem";
const BOOTSTRAP_CLIENT_KEY: &str = "bootstrap-client-key.pem";
const BOOTSTRAP_TIMESTAMP: &str = ".bootstrap-generated";

/// Generate bootstrap certificates on first boot
/// Returns Ok(()) if certificates already exist or were successfully generated
pub fn generate_bootstrap_certificates() -> Result<(), Box<dyn std::error::Error>> {
    let cert_dir = Path::new(BOOTSTRAP_CERT_DIR);
    
    // Create certificate directory
    fs::create_dir_all(cert_dir)?;

    // Check if certificates already exist
    if cert_dir.join(BOOTSTRAP_CA_CERT).exists() {
        info!("Bootstrap certificates already exist, skipping generation");
        return Ok(());
    }

    info!("Generating bootstrap certificates for first-time setup...");

    // Generate self-signed CA with 24-hour validity
    let ca_keypair = KeyPair::generate(&rcgen::PKCS_ECDSA_P256_SHA256)?;
    
    let mut ca_params = CertificateParams::new(vec!["KeelOS Bootstrap CA".to_string()]);
    ca_params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
    ca_params.key_usages = vec![
        rcgen::KeyUsagePurpose::KeyCertSign,
        rcgen::KeyUsagePurpose::CrlSign,
    ];
    
    let mut ca_dn = DistinguishedName::new();
    ca_dn.push(DnType::CommonName, "KeelOS Bootstrap CA");
    ca_dn.push(DnType::OrganizationName, "KeelOS");
    ca_params.distinguished_name = ca_dn;
    
    // Set 24-hour validity
    let now = std::time::SystemTime::now();
    let not_before = now.duration_since(UNIX_EPOCH)?.as_secs();
    let not_after = not_before + 86400; // 24 hours
    ca_params.not_before = time::OffsetDateTime::from_unix_timestamp(not_before as i64)?;
    ca_params.not_after = time::OffsetDateTime::from_unix_timestamp(not_after as i64)?;
    
    let ca_cert = Certificate::from_params(ca_params)?;
    let ca_cert_pem = ca_cert.serialize_pem()?;
    let ca_key_pem = ca_keypair.serialize_pem();

    // Generate initial client certificate with 24-hour validity
    let client_keypair = KeyPair::generate(&rcgen::PKCS_ECDSA_P256_SHA256)?;
    
    let mut client_params = CertificateParams::new(vec!["bootstrap-admin".to_string()]);
    client_params.key_usages = vec![
        rcgen::KeyUsagePurpose::DigitalSignature,
        rcgen::KeyUsagePurpose::KeyEncipherment,
    ];
    client_params.extended_key_usages = vec![rcgen::ExtendedKeyUsagePurpose::ClientAuth];
    
    let mut client_dn = DistinguishedName::new();
    client_dn.push(DnType::CommonName, "bootstrap-admin");
    client_params.distinguished_name = client_dn;
    
    // Set 24-hour validity
    client_params.not_before = time::OffsetDateTime::from_unix_timestamp(not_before as i64)?;
    client_params.not_after = time::OffsetDateTime::from_unix_timestamp(not_after as i64)?;
    
    let client_cert_pem = client_params.serialize_pem_with_signer(&ca_cert)?;
    let client_key_pem = client_keypair.serialize_pem();

    // Write certificates to disk
    fs::write(cert_dir.join(BOOTSTRAP_CA_CERT), &ca_cert_pem)?;
    fs::write(cert_dir.join(BOOTSTRAP_CA_KEY), &ca_key_pem)?;
    fs::write(cert_dir.join(BOOTSTRAP_CLIENT_CERT), &client_cert_pem)?;
    fs::write(cert_dir.join(BOOTSTRAP_CLIENT_KEY), &client_key_pem)?;

    // Write timestamp marker
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)?
        .as_secs();
    fs::write(cert_dir.join(BOOTSTRAP_TIMESTAMP), timestamp.to_string())?;

    info!("Bootstrap certificates generated successfully (valid for 24 hours)");
    
    // Display bootstrap instructions
    display_bootstrap_instructions();

    Ok(())
}

/// Display bootstrap instructions on console
fn display_bootstrap_instructions() {
    println!("\n╔══════════════════════════════════════════════════════════╗");
    println!("║        KeelOS Node - Bootstrap Required                 ║");
    println!("╚══════════════════════════════════════════════════════════╝\n");
    println!("🔐 To manage this node, first retrieve bootstrap certificates:\n");
    println!("   osctl init --bootstrap --node <this-node-ip>\n");
    println!("⏰ Bootstrap certificates expire in 24 hours");
    println!("   Complete setup before expiry!\n");
    println!("════════════════════════════════════════════════════════════\n");
}

/// Cleanup bootstrap certificates after successful Kubernetes join
pub fn cleanup_bootstrap_certificates() -> Result<(), Box<dyn std::error::Error>> {
    let cert_dir = Path::new(BOOTSTRAP_CERT_DIR);
    
    // Check if node has been bootstrapped to K8s
    if !Path::new("/var/lib/keel/kubernetes/kubelet.kubeconfig").exists() {
        return Ok(()); // Not bootstrapped yet, keep certs
    }
    
    info!("Node bootstrapped to Kubernetes - removing bootstrap certificates");
    
    // Remove bootstrap certificates (ignore errors)
    let _ = fs::remove_file(cert_dir.join(BOOTSTRAP_CA_CERT));
    let _ = fs::remove_file(cert_dir.join(BOOTSTRAP_CA_KEY));
    let _ = fs::remove_file(cert_dir.join(BOOTSTRAP_CLIENT_CERT));
    let _ = fs::remove_file(cert_dir.join(BOOTSTRAP_CLIENT_KEY));
    let _ = fs::remove_file(cert_dir.join(BOOTSTRAP_TIMESTAMP));
    
    info!("Bootstrap certificates removed");
    Ok(())
}
