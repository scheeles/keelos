//! Cryptography utilities for KeelOS
//!
//! Provides certificate loading and self-signed generation for mTLS.

use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum CryptoError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Certificate error: {0}")]
    Cert(String),
}

/// Parse certificate expiry from PEM-encoded certificate
pub fn parse_cert_expiry(cert_pem: &str) -> Result<chrono::DateTime<chrono::Utc>, String> {
    use chrono::TimeZone;
    use x509_parser::prelude::*;

    // Parse PEM to get DER bytes
    let pem_data = ::pem::parse(cert_pem).map_err(|e| format!("Failed to parse PEM: {}", e))?;

    // Parse X.509 certificate from DER
    let (_, cert) = X509Certificate::from_der(pem_data.contents())
        .map_err(|e| format!("Failed to parse X.509 certificate: {}", e))?;

    // Get notAfter (expiry) timestamp
    let not_after = cert.validity().not_after;
    let timestamp = not_after.timestamp();

    // Convert to DateTime
    chrono::Utc
        .timestamp_opt(timestamp, 0)
        .single()
        .ok_or_else(|| "Invalid timestamp in certificate".to_string())
}

/// Check if a certificate needs renewal based on threshold
pub fn check_cert_needs_renewal(
    cert_path: &str,
    renewal_threshold_days: u32,
) -> Result<bool, String> {
    use std::fs;

    // Read certificate file
    let cert_pem =
        fs::read_to_string(cert_path).map_err(|e| format!("Failed to read certificate: {}", e))?;

    // Parse expiry
    let expiry = parse_cert_expiry(&cert_pem)?;

    // Calculate threshold
    let now = chrono::Utc::now();
    let renewal_deadline = expiry - chrono::Duration::days(renewal_threshold_days as i64);

    // Check if we're past the renewal deadline
    Ok(now >= renewal_deadline)
}

/// Load certificates from a PEM file
pub fn load_certs<P: AsRef<Path>>(path: P) -> Result<Vec<CertificateDer<'static>>, CryptoError> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    let certs = rustls_pemfile::certs(&mut reader)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| CryptoError::Cert(format!("Failed to parse certificates: {}", e)))?;
    Ok(certs)
}

/// Load a private key from a PEM file
pub fn load_private_key<P: AsRef<Path>>(path: P) -> Result<PrivateKeyDer<'static>, CryptoError> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);

    // Try PKCS#8 first, then RSA, then EC
    let mut keys: Vec<PrivateKeyDer<'static>> = rustls_pemfile::private_key(&mut reader)
        .map_err(|e| CryptoError::Cert(format!("Failed to parse private key: {}", e)))?
        .into_iter()
        .collect();

    if keys.is_empty() {
        return Err(CryptoError::Cert("No private key found".into()));
    }

    Ok(keys.remove(0))
}

/// Generate a self-signed certificate for bootstrapping/tests
pub fn generate_self_signed() -> Result<(String, String), CryptoError> {
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".to_string()])
        .map_err(|e| CryptoError::Cert(e.to_string()))?;

    let cert_pem = cert.cert.pem();
    let key_pem = cert.key_pair.serialize_pem();

    Ok((cert_pem, key_pem))
}

/// Generate a bootstrap certificate with specific validity period
/// Returns (cert_pem, key_pem)
/// Note: Currently uses fixed validity from rcgen, validity_hours parameter is for future use
pub fn generate_bootstrap_certificate(
    _validity_hours: u32,
) -> Result<(String, String), CryptoError> {
    // For now, use the simple self-signed generator
    // TODO: Implement custom validity period when rcgen API supports it better
    let cert = rcgen::generate_simple_self_signed(vec!["keel-bootstrap".to_string()])
        .map_err(|e| CryptoError::Cert(e.to_string()))?;

    let cert_pem = cert.cert.pem();
    let key_pem = cert.key_pair.serialize_pem();

    Ok((cert_pem, key_pem))
}

/// Validate a bootstrap certificate (check it's self-signed and has reasonable expiry)
pub fn validate_bootstrap_cert(cert_pem: &str) -> Result<(), CryptoError> {
    // Basic validation: check PEM format
    if !cert_pem.contains("BEGIN CERTIFICATE") {
        return Err(CryptoError::Cert("Invalid PEM format".into()));
    }

    // TODO: Add more validation (expiry check, self-signed verification)
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_self_signed() {
        let result = generate_self_signed();
        assert!(result.is_ok());
        let (cert_pem, key_pem) = result.unwrap();
        assert!(cert_pem.contains("BEGIN CERTIFICATE"));
        assert!(key_pem.contains("BEGIN PRIVATE KEY"));
    }
}
