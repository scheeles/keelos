//! Cryptography utilities for KeelOS
//!
//! Provides certificate loading and self-signed generation for mTLS.

use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::BufReader;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use thiserror::Error;
use x509_parser::prelude::*;

#[derive(Error, Debug)]
pub enum CryptoError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Certificate error: {0}")]
    Cert(String),
    #[error("X509 parsing error: {0}")]
    X509Parse(String),
    #[error("rcgen error: {0}")]
    RcgenError(#[from] rcgen::Error),
}

/// Certificate information for display
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CertificateInfo {
    pub exists: bool,
    pub subject: String,
    pub issuer: String,
    pub not_before: String,
    pub not_after: String,
    pub is_expired: bool,
    pub days_until_expiry: i64,
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

/// Generate a bootstrap certificate with specified validity hours
pub fn generate_bootstrap_cert(
    common_name: &str,
    validity_hours: u32,
) -> Result<(String, String), CryptoError> {
    use rcgen::*;

    let mut params = CertificateParams::new(vec![common_name.to_string()])
        .map_err(|e| CryptoError::Cert(e.to_string()))?;

    // Set validity period
    let now = ::time::OffsetDateTime::now_utc();
    let not_before = now - ::time::Duration::minutes(5); // 5 min grace period
    let not_after = now + ::time::Duration::hours(validity_hours as i64);

    params.not_before = not_before;
    params.not_after = not_after;

    // Add subject alternative names
    params.subject_alt_names = vec![
        SanType::DnsName(common_name.try_into().map_err(|e: rcgen::Error| CryptoError::Cert(e.to_string()))?),
        SanType::DnsName("localhost".try_into().map_err(|e: rcgen::Error| CryptoError::Cert(e.to_string()))?),
        SanType::IpAddress(std::net::IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1))),
    ];

    // Set distinguished name
    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, common_name);
    dn.push(DnType::OrganizationName, "KeelOS");
    dn.push(DnType::OrganizationalUnitName, "Bootstrap");
    params.distinguished_name = dn;

    let key_pair = KeyPair::generate()?;
    let cert = params.self_signed(&key_pair)?;

    let cert_pem = cert.pem();
    let key_pem = key_pair.serialize_pem();

    Ok((cert_pem, key_pem))
}

/// Generate a CSR (Certificate Signing Request)
pub fn generate_csr(common_name: &str, org: &str) -> Result<(String, String), CryptoError> {
    use rcgen::*;

    let mut params = CertificateParams::new(vec![common_name.to_string()])
        .map_err(|e| CryptoError::Cert(e.to_string()))?;

    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, common_name);
    dn.push(DnType::OrganizationName, org);
    params.distinguished_name = dn;

    let key_pair = KeyPair::generate()?;
    let csr_pem = params
        .serialize_request(&key_pair)?
        .pem()
        .map_err(|e| CryptoError::Cert(e.to_string()))?;

    let key_pem = key_pair.serialize_pem();

    Ok((csr_pem, key_pem))
}

/// Get certificate information from a PEM file
pub fn get_certificate_info<P: AsRef<Path>>(path: P) -> Result<CertificateInfo, CryptoError> {
    let path_ref = path.as_ref();

    if !path_ref.exists() {
        return Ok(CertificateInfo {
            exists: false,
            subject: String::new(),
            issuer: String::new(),
            not_before: String::new(),
            not_after: String::new(),
            is_expired: true,
            days_until_expiry: 0,
        });
    }

    let pem_data = fs::read_to_string(path_ref)?;
    let pem = ::pem::parse(&pem_data).map_err(|e: ::pem::PemError| CryptoError::Cert(e.to_string()))?;

    let (_, cert) = X509Certificate::from_der(pem.contents())
        .map_err(|e| CryptoError::X509Parse(e.to_string()))?;

    let subject = cert.subject().to_string();
    let issuer = cert.issuer().to_string();

    let validity = cert.validity();
    let not_before = validity.not_before.to_rfc2822();
    let not_after = validity.not_after.to_rfc2822();

    // Check expiry
    let now = ::time::OffsetDateTime::now_utc();
    let expiry_time = ::time::OffsetDateTime::from_unix_timestamp(validity.not_after.timestamp())
        .map_err(|e| CryptoError::Cert(e.to_string()))?;

    let is_expired = now > expiry_time;
    let duration_until_expiry = expiry_time - now;
    let days_until_expiry = duration_until_expiry.whole_days();

    Ok(CertificateInfo {
        exists: true,
        subject,
        issuer,
        not_before: not_before?,
        not_after: not_after?,
        is_expired,
        days_until_expiry,
    })
}

/// Verify if a certificate is expired
pub fn verify_certificate_expiry<P: AsRef<Path>>(path: P) -> Result<bool, CryptoError> {
    let info = get_certificate_info(path)?;
    Ok(!info.is_expired)
}

/// Set file permissions to 0600 (owner read/write only)
pub fn set_key_permissions<P: AsRef<Path>>(path: P) -> Result<(), CryptoError> {
    let metadata = fs::metadata(path.as_ref())?;
    let mut permissions = metadata.permissions();
    permissions.set_mode(0o600);
    fs::set_permissions(path, permissions)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_generate_self_signed() {
        let result = generate_self_signed();
        assert!(result.is_ok());
        let (cert_pem, key_pem) = result.unwrap();
        assert!(cert_pem.contains("BEGIN CERTIFICATE"));
        assert!(key_pem.contains("BEGIN PRIVATE KEY"));
    }

    #[test]
    fn test_generate_bootstrap_cert() {
        let result = generate_bootstrap_cert("test-node", 24);
        assert!(result.is_ok());
        let (cert_pem, key_pem) = result.unwrap();
        assert!(cert_pem.contains("BEGIN CERTIFICATE"));
        assert!(key_pem.contains("BEGIN PRIVATE KEY"));
    }

    #[test]
    fn test_generate_csr() {
        let result = generate_csr("test-client", "KeelOS");
        assert!(result.is_ok());
        let (csr_pem, key_pem) = result.unwrap();
        assert!(csr_pem.contains("BEGIN CERTIFICATE REQUEST"));
        assert!(key_pem.contains("BEGIN PRIVATE KEY"));
    }

    #[test]
    fn test_get_certificate_info_nonexistent() {
        let result = get_certificate_info("/tmp/nonexistent-cert.pem");
        assert!(result.is_ok());
        let info = result.unwrap();
        assert!(!info.exists);
    }

    #[test]
    fn test_get_certificate_info() {
        let (cert_pem, _) = generate_bootstrap_cert("test", 24).unwrap();
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(cert_pem.as_bytes()).unwrap();
        temp_file.flush().unwrap();

        let result = get_certificate_info(temp_file.path());
        assert!(result.is_ok());
        let info = result.unwrap();
        assert!(info.exists);
        assert!(!info.is_expired);
        assert!(info.days_until_expiry > 0);
    }

    #[test]
    fn test_set_key_permissions() {
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(b"test key").unwrap();
        temp_file.flush().unwrap();

        let result = set_key_permissions(temp_file.path());
        assert!(result.is_ok());

        let metadata = fs::metadata(temp_file.path()).unwrap();
        let permissions = metadata.permissions();
        assert_eq!(permissions.mode() & 0o777, 0o600);
    }
}
