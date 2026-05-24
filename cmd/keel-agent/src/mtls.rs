//! mTLS configuration with dual-CA support
//!
//! Manages TLS setup for the agent to accept both:
//! - Bootstrap certificates (self-signed, 24h)
//! - Operational certificates (K8s-signed, 365d)
//!
//! # Security Architecture
//!
//! The main gRPC server uses `client_auth_required` — all clients MUST
//! present a valid certificate signed by a trusted CA. The `InitBootstrap`
//! endpoint runs on a separate listener with optional client auth to allow
//! initial certificate exchange.

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

    /// Build TLS configuration with dual-CA support (required client auth)
    ///
    /// # Errors
    ///
    /// Returns an error if the server certificate or key files cannot be read.
    pub fn build_tls_config(&self) -> Result<ServerTlsConfig, Box<dyn std::error::Error>> {
        let cert_pem = fs::read_to_string(&self.server_cert_path)?;
        let key_pem = fs::read_to_string(&self.server_key_path)?;
        let identity = Identity::from_pem(cert_pem, key_pem);

        let mut tls_config = ServerTlsConfig::new().identity(identity);

        // Load all trusted CA certificates (bootstrap + operational)
        let ca_certs = self.load_ca_certificates();
        let combined_ca = ca_certs.join("\n");

        if !combined_ca.is_empty() {
            // SECURITY: Require client certificates for the main gRPC server.
            // All clients MUST present a valid certificate signed by a trusted CA.
            // The InitBootstrap endpoint should use a separate listener with
            // build_bootstrap_tls_config() which allows optional client auth.
            tls_config =
                tls_config.client_ca_root(tonic::transport::Certificate::from_pem(combined_ca));
            info!(
                "Configured dual-CA mTLS with {} CA certificates (required client auth)",
                ca_certs.len()
            );
        } else {
            warn!("No CA certificates loaded - mTLS will not verify client identity!");
        }

        Ok(tls_config)
    }

    /// Build TLS configuration for the bootstrap listener.
    ///
    /// This uses `client_auth_optional(true)` to allow the `InitBootstrap`
    /// endpoint to accept connections without a client certificate. Only the
    /// bootstrap listener should use this configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if the server certificate or key cannot be read.
    pub fn build_bootstrap_tls_config(
        &self,
    ) -> Result<ServerTlsConfig, Box<dyn std::error::Error>> {
        let cert_pem = fs::read_to_string(&self.server_cert_path)?;
        let key_pem = fs::read_to_string(&self.server_key_path)?;
        let identity = Identity::from_pem(cert_pem, key_pem);

        let mut tls_config = ServerTlsConfig::new().identity(identity);

        // Load CA certs for optional verification
        let ca_certs = self.load_ca_certificates();
        let combined_ca = ca_certs.join("\n");

        if !combined_ca.is_empty() {
            // SECURITY: Optional client auth — only for the bootstrap listener.
            // This allows InitBootstrap to be called without a client cert.
            tls_config = tls_config
                .client_ca_root(tonic::transport::Certificate::from_pem(combined_ca))
                .client_auth_optional(true);
        }

        info!("Configured bootstrap TLS with optional client auth");
        Ok(tls_config)
    }

    /// Check if TLS can be configured (server cert exists)
    pub fn can_configure(&self) -> bool {
        Path::new(&self.server_cert_path).exists() && Path::new(&self.server_key_path).exists()
    }

    /// Load all CA certificates from bootstrap dir and operational CA.
    fn load_ca_certificates(&self) -> Vec<String> {
        let mut ca_certs = Vec::new();

        if Path::new(&self.bootstrap_ca_dir).exists() {
            if let Ok(entries) = fs::read_dir(&self.bootstrap_ca_dir) {
                for entry in entries.flatten() {
                    if entry.path().extension().and_then(|s| s.to_str()) == Some("pem") {
                        match fs::read_to_string(entry.path()) {
                            Ok(cert_pem) => {
                                ca_certs.push(cert_pem);
                            }
                            Err(e) => {
                                warn!("Failed to read CA {}: {}", entry.path().display(), e);
                            }
                        }
                    }
                }
            }
        }

        if let Some(ref ca_path) = self.operational_ca_path {
            if Path::new(ca_path).exists() {
                if let Ok(cert_pem) = fs::read_to_string(ca_path) {
                    ca_certs.push(cert_pem);
                }
            }
        }

        ca_certs
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
