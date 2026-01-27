//! Certificate rotation logic for KeelOS
//!
//! Handles automatic certificate rotation before expiry.

use crate::CryptoError;
use std::path::Path;
use thiserror::Error;
use time::OffsetDateTime;

#[derive(Error, Debug)]
pub enum RotationError {
    #[error("Crypto error: {0}")]
    Crypto(#[from] CryptoError),
    #[error("Certificate parse error: {0}")]
    Parse(String),
    #[error("Time error: {0}")]
    Time(String),
}

/// Certificate expiry information
#[derive(Debug, Clone)]
pub struct ExpiryInfo {
    pub not_before: OffsetDateTime,
    pub not_after: OffsetDateTime,
    pub days_until_expiry: i64,
    pub is_expiring_soon: bool,
}

/// Check if a certificate is expiring soon
pub fn check_expiry(cert_pem: &str, warn_days: u32) -> Result<ExpiryInfo, RotationError> {
    // Parse the PEM certificate
    let pem_data = pem::parse(cert_pem)
        .map_err(|e| RotationError::Parse(format!("Failed to parse PEM: {}", e)))?;

    // Use x509-parser to extract validity dates
    use x509_parser::prelude::*;

    let (_, cert) = X509Certificate::from_der(pem_data.contents())
        .map_err(|e| RotationError::Parse(format!("Failed to parse X.509: {:?}", e)))?;

    let not_before_timestamp = cert.validity().not_before.timestamp();
    let not_after_timestamp = cert.validity().not_after.timestamp();

    let not_before = OffsetDateTime::from_unix_timestamp(not_before_timestamp)
        .map_err(|e| RotationError::Time(format!("Invalid not_before: {}", e)))?;

    let not_after = OffsetDateTime::from_unix_timestamp(not_after_timestamp)
        .map_err(|e| RotationError::Time(format!("Invalid not_after: {}", e)))?;

    let now = OffsetDateTime::now_utc();
    let days_until_expiry = (not_after - now).whole_days();
    let is_expiring_soon = days_until_expiry <= warn_days as i64;

    Ok(ExpiryInfo {
        not_before,
        not_after,
        days_until_expiry,
        is_expiring_soon,
    })
}

/// Atomically rotate certificate files
/// 
/// This function writes new certificate and key to temporary files,
/// verifies they are valid, then atomically renames them to replace
/// the old certificates.
pub fn rotate_certificate<P: AsRef<Path>>(
    cert_path: P,
    key_path: P,
    new_cert_pem: &str,
    new_key_pem: &str,
) -> Result<(), RotationError> {
    use std::fs;

    let cert_path = cert_path.as_ref();
    let key_path = key_path.as_ref();

    // Create temp paths
    let temp_cert = cert_path.with_extension("pem.new");
    let temp_key = key_path.with_extension("key.new");

    // Write to temp files
    fs::write(&temp_cert, new_cert_pem)
        .map_err(|e| RotationError::Crypto(CryptoError::Io(e)))?;
    fs::write(&temp_key, new_key_pem)
        .map_err(|e| RotationError::Crypto(CryptoError::Io(e)))?;

    // Verify the new certificate is valid
    let _ = check_expiry(new_cert_pem, 0)?;

    // Atomic rename
    fs::rename(&temp_cert, cert_path)
        .map_err(|e| RotationError::Crypto(CryptoError::Io(e)))?;
    fs::rename(&temp_key, key_path)
        .map_err(|e| RotationError::Crypto(CryptoError::Io(e)))?;

    Ok(())
}

/// Schedule certificate rotation
/// 
/// Returns the duration until the next rotation check should occur.
pub fn schedule_rotation(
    cert_pem: &str,
    rotation_days_before_expiry: u32,
) -> Result<std::time::Duration, RotationError> {
    let expiry = check_expiry(cert_pem, rotation_days_before_expiry)?;

    if expiry.is_expiring_soon {
        // Rotate immediately
        return Ok(std::time::Duration::from_secs(0));
    }

    // Calculate when to check again
    let days_until_rotation = expiry.days_until_expiry - (rotation_days_before_expiry as i64);

    if days_until_rotation <= 0 {
        return Ok(std::time::Duration::from_secs(0));
    }

    // Check daily if we're getting close, otherwise check weekly
    let check_interval = if days_until_rotation <= 7 {
        std::time::Duration::from_secs(86400) // 1 day
    } else {
        std::time::Duration::from_secs(86400 * 7) // 1 week
    };

    Ok(check_interval)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ca::CertificateAuthority;

    #[test]
    fn test_check_expiry() {
        let ca = CertificateAuthority::generate_root_ca("Test CA", 365).unwrap();
        let (cert_pem, _) = ca.issue_certificate("test.example.com", 90, true).unwrap();

        let expiry = check_expiry(&cert_pem, 30).unwrap();
        assert!(expiry.days_until_expiry > 80);
        assert!(expiry.days_until_expiry < 95);
        assert!(!expiry.is_expiring_soon);
    }

    #[test]
    fn test_expiring_soon() {
        let ca = CertificateAuthority::generate_root_ca("Test CA", 365).unwrap();
        let (cert_pem, _) = ca.issue_certificate("test.example.com", 10, true).unwrap();

        let expiry = check_expiry(&cert_pem, 30).unwrap();
        assert!(expiry.is_expiring_soon);
    }

    #[test]
    fn test_rotate_certificate() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let cert_path = temp_dir.path().join("server.pem");
        let key_path = temp_dir.path().join("server.key");

        let ca = CertificateAuthority::generate_root_ca("Test CA", 365).unwrap();
        let (old_cert, old_key) = ca.issue_certificate("old.example.com", 90, true).unwrap();

        // Write old cert
        std::fs::write(&cert_path, &old_cert).unwrap();
        std::fs::write(&key_path, &old_key).unwrap();

        // Rotate to new cert
        let (new_cert, new_key) = ca.issue_certificate("new.example.com", 90, true).unwrap();
        rotate_certificate(&cert_path, &key_path, &new_cert, &new_key).unwrap();

        // Verify rotation
        let rotated_cert = std::fs::read_to_string(&cert_path).unwrap();
        assert_eq!(rotated_cert, new_cert);
    }

    #[test]
    fn test_schedule_rotation() {
        let ca = CertificateAuthority::generate_root_ca("Test CA", 365).unwrap();
        let (cert_pem, _) = ca.issue_certificate("test.example.com", 90, true).unwrap();

        let duration = schedule_rotation(&cert_pem, 30).unwrap();
        // Should schedule for sometime in the future
        assert!(duration.as_secs() > 0);
    }
}
