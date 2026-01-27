//! Cryptography utilities for KeelOS
//!
//! Provides certificate loading and self-signed generation for mTLS.

pub mod ca;
pub mod rotation;

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
