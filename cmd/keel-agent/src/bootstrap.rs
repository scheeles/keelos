use openssl::hash::MessageDigest;
use openssl::pkey::PKey;
use openssl::x509::{X509Req, X509};
use std::fs;
use std::path::Path;
use tracing::info;

const BOOTSTRAP_CERT_DIR: &str = "/etc/keel/crypto";
const BOOTSTRAP_CA_CERT: &str = "bootstrap-ca.pem";
const BOOTSTRAP_CA_KEY: &str = "bootstrap-ca-key.pem";

/// Sign a client CSR with the bootstrap CA
/// Returns the signed certificate in PEM format
pub fn sign_bootstrap_csr(csr_pem: &str) -> Result<String, Box<dyn std::error::Error>> {
    // Load bootstrap CA certificate and key
    let ca_cert_path = Path::new(BOOTSTRAP_CERT_DIR).join(BOOTSTRAP_CA_CERT);
    let ca_key_path = Path::new(BOOTSTRAP_CERT_DIR).join(BOOTSTRAP_CA_KEY);

    let ca_cert_pem = fs::read_to_string(&ca_cert_path)?;
    let ca_key_pem = fs::read_to_string(&ca_key_path)?;

    let ca_cert = X509::from_pem(ca_cert_pem.as_bytes())?;
    let ca_key = PKey::private_key_from_pem(ca_key_pem.as_bytes())?;

    // Parse the CSR
    let csr = X509Req::from_pem(csr_pem.as_bytes())?;

    // Verify CSR signature
    let csr_pubkey = csr.public_key()?;
    if !csr.verify(&csr_pubkey)? {
        return Err("Invalid CSR signature".into());
    }

    // Build certificate from CSR
    let mut cert_builder = X509::builder()?;

    // Set version (v3 = 2 in OpenSSL)
    cert_builder.set_version(2)?;

    // Set serial number (simple increment, in production use random)
    let serial = openssl::bn::BigNum::from_u32(1)?;
    let serial_asn1 = serial.to_asn1_integer()?;
    cert_builder.set_serial_number(serial_asn1.as_ref())?;

    // Copy subject from CSR
    cert_builder.set_subject_name(csr.subject_name())?;

    // Set issuer to CA
    cert_builder.set_issuer_name(ca_cert.subject_name())?;

    // Set public key from CSR
    cert_builder.set_pubkey(&csr_pubkey)?;

    // Set validity period (24 hours)
    let not_before = openssl::asn1::Asn1Time::days_from_now(0)?;
    let not_after = openssl::asn1::Asn1Time::days_from_now(1)?;
    cert_builder.set_not_before(&not_before)?;
    cert_builder.set_not_after(&not_after)?;

    // Add extensions for client certificate
    let context = cert_builder.x509v3_context(Some(&ca_cert), None);
    let ext_key_usage = openssl::x509::extension::ExtendedKeyUsage::new()
        .client_auth()
        .build()?;
    cert_builder.append_extension(ext_key_usage)?;

    let key_usage = openssl::x509::extension::KeyUsage::new()
        .digital_signature()
        .key_encipherment()
        .build()?;
    cert_builder.append_extension(key_usage)?;

    // Sign the certificate
    cert_builder.sign(&ca_key, MessageDigest::sha256())?;

    let cert = cert_builder.build();
    let cert_pem = String::from_utf8(cert.to_pem()?)?;

    info!("Signed bootstrap client certificate from CSR");

    Ok(cert_pem)
}

/// Check if node has been bootstrapped to Kubernetes
pub fn is_bootstrapped() -> bool {
    Path::new("/var/lib/keel/kubernetes/kubelet.kubeconfig").exists()
}

/// Get bootstrap CA certificate
pub fn get_bootstrap_ca() -> Result<String, Box<dyn std::error::Error>> {
    let ca_cert_path = Path::new(BOOTSTRAP_CERT_DIR).join(BOOTSTRAP_CA_CERT);
    Ok(fs::read_to_string(&ca_cert_path)?)
}
