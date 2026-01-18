use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use rustls::{Certificate, PrivateKey};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum CryptoError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Certificate error: {0}")]
    Cert(String),
}

pub fn load_certs<P: AsRef<Path>>(path: P) -> Result<Vec<Certificate>, CryptoError> {
    let mut reader = BufReader::new(File::open(path)?);
    let certs = rustls_pemfile::certs(&mut reader)
        .map_err(|_| CryptoError::Cert("Failed to parse certificates".into()))?
        .into_iter()
        .map(Certificate)
        .collect();
    Ok(certs)
}

pub fn load_private_key<P: AsRef<Path>>(path: P) -> Result<PrivateKey, CryptoError> {
    let mut reader = BufReader::new(File::open(path)?);
    let mut keys = rustls_pemfile::pkcs8_private_keys(&mut reader)
        .map_err(|_| CryptoError::Cert("Failed to parse private keys".into()))?;
    
    if keys.is_empty() {
        return Err(CryptoError::Cert("No private key found".into()));
    }
    
    Ok(PrivateKey(keys.remove(0)))
}

// Helper to generate a self-signed cert for bootstrapping/tests
pub fn generate_self_signed() -> Result<(String, String), CryptoError> {
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".to_string()])
        .map_err(|e| CryptoError::Cert(e.to_string()))?;
    
    Ok((cert.serialize_pem().unwrap(), cert.serialize_private_key_pem()))
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
