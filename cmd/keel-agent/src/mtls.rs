//! mTLS configuration with dual-CA support
//!
//! Manages TLS setup for the agent to accept both:
//! - Bootstrap certificates (self-signed, 24h)
//! - Operational certificates (K8s-signed, 365d)

use std::fs;
use std::path::Path;
use tonic::transport::{Identity, ServerTlsConfig};
use tracing::{info, warn};

pub struct TlsManager {
    server_cert_path: String,
    server_key_path: String,
    bootstrap_ca_dir: String,
    operational_ca_path: Option<String>,
}

impl TlsManager {
    pub fn new(
        server_cert_path: String,
        server_key_path: String,
        bootstrap_ca_dir: String,
        operational_ca_path: Option<String>,
    ) -> Self {
        Self {
            server_cert_path,
            server_key_path,
            bootstrap_ca_dir,
            operational_ca_path,
        }
    }

    /// Build TLS configuration with dual-CA support
    pub fn build_tls_config(&self) -> Result<ServerTlsConfig, Box<dyn std::error::Error>> {
        // Load server's certificate and key
        let cert_pem = fs::read_to_string(&self.server_cert_path)?;
        let key_pem = fs::read_to_string(&self.server_key_path)?;

        let identity = Identity::from_pem(cert_pem, key_pem);

        let mut tls_config = ServerTlsConfig::new().identity(identity);

        // Load all bootstrap CA certificates (each client's self-signed cert)
        let mut ca_certs = Vec::new();

        if Path::new(&self.bootstrap_ca_dir).exists() {
            for entry in fs::read_dir(&self.bootstrap_ca_dir)? {
                let entry = entry?;
                if entry.path().extension().and_then(|s| s.to_str()) == Some("pem") {
                    match fs::read_to_string(entry.path()) {
                        Ok(cert_pem) => {
                            ca_certs.push(cert_pem);
                            info!("Loaded bootstrap CA: {}", entry.path().display());
                        }
                        Err(e) => {
                            warn!(
                                "Failed to read bootstrap CA {}: {}",
                                entry.path().display(),
                                e
                            );
                        }
                    }
                }
            }
        }

        // Load operational CA if present (K8s cluster CA)
        if let Some(ref ca_path) = self.operational_ca_path {
            if Path::new(ca_path).exists() {
                match fs::read_to_string(ca_path) {
                    Ok(cert_pem) => {
                        ca_certs.push(cert_pem);
                        info!("Loaded operational CA: {}", ca_path);
                    }
                    Err(e) => {
                        warn!("Failed to read operational CA: {}", e);
                    }
                }
            }
        }

        // Combine all CA certificates
        let combined_ca = ca_certs.join("\n");

        if !combined_ca.is_empty() {
            // Configure client CA but make it OPTIONAL
            // This allows InitBootstrap to be called without a client cert
            // while still verifying certs when they are presented
            tls_config = tls_config
                .client_ca_root(tonic::transport::Certificate::from_pem(combined_ca))
                .client_auth_optional(true); // KEY: Make client auth optional
            info!(
                "Configured dual-CA mTLS with {} CA certificates (optional client auth)",
                ca_certs.len()
            );
        } else {
            warn!("No CA certificates loaded - mTLS will not work!");
        }

        Ok(tls_config)
    }

    /// Check if TLS can be configured (server cert exists)
    pub fn can_configure(&self) -> bool {
        Path::new(&self.server_cert_path).exists() && Path::new(&self.server_key_path).exists()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tls_manager_creation() {
        let manager = TlsManager::new(
            "/var/lib/keel/crypto/server.pem".to_string(),
            "/var/lib/keel/crypto/server.key".to_string(),
            "/var/lib/keel/crypto/trusted-clients/bootstrap".to_string(),
            Some("/var/lib/keel/crypto/ca.pem".to_string()),
        );

        // Just verify it was created
        assert_eq!(manager.server_cert_path, "/var/lib/keel/crypto/server.pem");
    }
}
