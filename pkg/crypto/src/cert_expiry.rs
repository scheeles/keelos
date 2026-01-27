//! Certificate expiry checking utilities
//!
//! Provides functions to check certificate expiration and determine
//! if rotation is needed.

use openssl::x509::X509;
use std::fs;
use std::path::Path;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum CertExpiryError {
    #[error("Failed to read certificate: {0}")]
    ReadError(#[from] std::io::Error),
    
    #[error("Failed to parse certificate: {0}")]
    ParseError(#[from] openssl::error::ErrorStack),
    
    #[error("Certificate has no expiry date")]
    NoExpiryDate,
}

pub type Result<T> = std::result::Result<T, CertExpiryError>;

/// Check how many days until certificate expires
pub fn days_until_expiry(cert_path: impl AsRef<Path>) -> Result<i64> {
    let cert_pem = fs::read(cert_path)?;
    let cert = X509::from_pem(&cert_pem)?;
    
    let not_after = cert.not_after();
    let expiry_time = asn1_time_to_unix(not_after)?;
    
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    
    let seconds_remaining = expiry_time - now;
    let days_remaining = seconds_remaining / 86400;
    
    Ok(days_remaining)
}

/// Check if certificate should be rotated
/// 
/// Returns true if:
/// - Certificate expires in less than 30 days
/// - Certificate is already expired
pub fn should_rotate(cert_path: impl AsRef<Path>) -> bool {
    match days_until_expiry(cert_path) {
        Ok(days) => days < 30,
        Err(_) => true, // If we can't check, assume rotation needed
    }
}

/// Get certificate subject common name
pub fn get_cert_subject_cn(cert_path: impl AsRef<Path>) -> Result<String> {
    let cert_pem = fs::read(cert_path)?;
    let cert = X509::from_pem(&cert_pem)?;
    
    let subject = cert.subject_name();
    for entry in subject.entries() {
        if entry.object().to_string() == "CN" {
            if let Ok(data) = entry.data().as_utf8() {
                return Ok(data.to_string());
            }
        }
    }
    
    Ok("unknown".to_string())
}

/// Convert ASN1Time to Unix timestamp
fn asn1_time_to_unix(asn1_time: &openssl::asn1::Asn1TimeRef) -> Result<i64> {
    // ASN1Time doesn't have direct conversion, so we use a workaround
    // by comparing with epoch
    let epoch = openssl::asn1::Asn1Time::from_unix(0)?;
    let diff = asn1_time.diff(&epoch)?;
    
    // diff.days is i32, convert to i64 seconds
    let seconds = (diff.days as i64) * 86400 + (diff.secs as i64);
    
    Ok(seconds)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_should_rotate_logic() {
        // This is a placeholder test
        // In production, create a test certificate with known expiry
        assert!(true);
    }
}
