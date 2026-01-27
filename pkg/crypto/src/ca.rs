//! Certificate Authority management for KeelOS
//!
//! Provides CA certificate generation, certificate issuance, and verification.

use rcgen::{
    BasicConstraints, Certificate, CertificateParams, DnType, IsCa, KeyPair, KeyUsagePurpose,
};
use std::fs;
use std::path::Path;
use thiserror::Error;
use time::{Duration, OffsetDateTime};

#[derive(Error, Debug)]
pub enum CaError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Certificate generation error: {0}")]
    CertGen(String),
    #[error("Certificate parse error: {0}")]
    Parse(String),
}

/// Certificate Authority for issuing and managing certificates
pub struct CertificateAuthority {
    ca_cert: Certificate,
    ca_cert_pem: String,
}

impl CertificateAuthority {
    /// Generate a new root CA certificate
    pub fn generate_root_ca(common_name: &str, validity_days: u32) -> Result<Self, CaError> {
        let mut params = CertificateParams::default();
        params.distinguished_name.push(DnType::CommonName, common_name);
        params.distinguished_name.push(DnType::OrganizationName, "KeelOS");
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        params.key_usages = vec![
            KeyUsagePurpose::KeyCertSign,
            KeyUsagePurpose::CrlSign,
            KeyUsagePurpose::DigitalSignature,
        ];

        let not_before = OffsetDateTime::now_utc();
        let not_after = not_before + Duration::days(validity_days as i64);
        params.not_before = not_before;
        params.not_after = not_after;

        // Use ECDSA P-256 signature algorithm
        let ca_cert = Certificate::generate_self_signed(params)
            .map_err(|e| CaError::CertGen(format!("Failed to generate CA certificate: {}", e)))?;

        let ca_cert_pem = ca_cert.pem();

        Ok(Self {
            ca_cert,
            ca_cert_pem,
        })
    }

    /// Load an existing CA certificate and key from PEM files
    pub fn load_from_pem<P: AsRef<Path>>(
        cert_path: P,
        key_path: P,
    ) -> Result<Self, CaError> {
        let cert_pem = fs::read_to_string(cert_path)?;
        let key_pem = fs::read_to_string(key_path)?;

        let key_pair = KeyPair::from_pem(&key_pem)
            .map_err(|e| CaError::Parse(format!("Failed to parse CA private key: {}", e)))?;

        let params = CertificateParams::from_ca_cert_pem(&cert_pem)
            .map_err(|e| CaError::Parse(format!("Failed to parse CA certificate: {}", e)))?;

        let ca_cert = params.self_signed(&key_pair)
            .map_err(|e| CaError::Parse(format!("Failed to load CA certificate: {}", e)))?;

        Ok(Self {
            ca_cert,
            ca_cert_pem: cert_pem,
        })
    }

    /// Issue a new certificate signed by this CA
    pub fn issue_certificate(
        &self,
        common_name: &str,
        validity_days: u32,
        is_server: bool,
    ) -> Result<(String, String), CaError> {
        let mut params = CertificateParams::default();
        params.distinguished_name.push(DnType::CommonName, common_name);
        params.distinguished_name.push(DnType::OrganizationName, "KeelOS");

        if is_server {
            params.key_usages = vec![
                KeyUsagePurpose::DigitalSignature,
                KeyUsagePurpose::KeyEncipherment,
            ];
            params.extended_key_usages = vec![
                rcgen::ExtendedKeyUsagePurpose::ServerAuth,
                rcgen::ExtendedKeyUsagePurpose::ClientAuth,
            ];
        } else {
            params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
            params.extended_key_usages = vec![rcgen::ExtendedKeyUsagePurpose::ClientAuth];
        }

        let not_before = OffsetDateTime::now_utc();
        let not_after = not_before + Duration::days(validity_days as i64);
        params.not_before = not_before;
        params.not_after = not_after;

        let cert = Certificate::generate(params, &self.ca_cert).map_err(|e| {
            CaError::CertGen(format!("Failed to generate certificate: {}", e))
        })?;

        let cert_pem = cert.pem();
        let key_pem = cert.key_pair().serialize_pem();

        Ok((cert_pem, key_pem))
    }

    /// Save CA certificate and key to files
    pub fn save_to_pem<P: AsRef<Path>>(
        &self,
        cert_path: P,
        key_path: P,
    ) -> Result<(), CaError> {
        fs::write(&cert_path, &self.ca_cert_pem)?;

        let key_pem = self.ca_cert.key_pair().serialize_pem();
        fs::write(&key_path, key_pem)?;

        Ok(())
    }

    /// Get the CA certificate in PEM format
    pub fn ca_cert_pem(&self) -> &str {
        &self.ca_cert_pem
    }

    /// Verify a certificate was signed by this CA
    pub fn verify_certificate(&self, cert_pem: &str) -> Result<bool, CaError> {
        // Load the certificate to verify
        let cert_der = pem::parse(cert_pem)
            .map_err(|e| CaError::Parse(format!("Failed to parse certificate PEM: {}", e)))?;

        // In a production implementation, we would use rustls or similar
        // to verify the signature chain. For now, we do a basic structural check.
        // This is a placeholder that should be replaced with proper verification.
        
        // TODO: Implement proper certificate chain verification
        // For now, we just check that it's a valid certificate structure
        Ok(cert_der.tag() == "CERTIFICATE")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_root_ca() {
        let ca = CertificateAuthority::generate_root_ca("Test CA", 365).unwrap();
        assert!(ca.ca_cert_pem().contains("BEGIN CERTIFICATE"));
    }

    #[test]
    fn test_issue_certificate() {
        let ca = CertificateAuthority::generate_root_ca("Test CA", 365).unwrap();
        let (cert_pem, key_pem) = ca.issue_certificate("test.example.com", 90, true).unwrap();

        assert!(cert_pem.contains("BEGIN CERTIFICATE"));
        assert!(key_pem.contains("BEGIN PRIVATE KEY"));
    }

    #[test]
    fn test_save_and_load_ca() {
        use std::fs;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let cert_path = temp_dir.path().join("ca.pem");
        let key_path = temp_dir.path().join("ca.key");

        let ca = CertificateAuthority::generate_root_ca("Test CA", 365).unwrap();
        ca.save_to_pem(&cert_path, &key_path).unwrap();

        let loaded_ca = CertificateAuthority::load_from_pem(&cert_path, &key_path).unwrap();
        assert_eq!(ca.ca_cert_pem(), loaded_ca.ca_cert_pem());
    }
}
