use std::fs;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;
use tonic::{Request, Status};
use tracing::{debug, info};

/// Represents which certificate tier is currently active
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CertificateTier {
    Bootstrap,
    Operational,
    Dual, // Both bootstrap and operational accepted during transition
}

/// Manages trusted client certificates and CA verification
pub struct TlsManager {
    /// Current certificate tier mode
    tier: Arc<RwLock<CertificateTier>>,
    /// Trusted client certificates (fingerprints or full certs)
    trusted_clients: Arc<RwLock<Vec<Vec<u8>>>>,
}

impl TlsManager {
    pub fn new() -> Self {
        Self {
            tier: Arc::new(RwLock::new(CertificateTier::Bootstrap)),
            trusted_clients: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Load trusted client certificates from directory
    pub async fn load_trusted_clients(&self, dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
        if !dir.exists() {
            info!("Trusted clients directory does not exist: {:?}", dir);
            return Ok(());
        }

        let mut loaded = 0;
        let bootstrap_dir = dir.join("bootstrap");
        let operational_dir = dir.join("operational");

        // Load bootstrap client certs
        if bootstrap_dir.exists() {
            for entry in fs::read_dir(&bootstrap_dir)? {
                let entry = entry?;
                if entry.path().extension().and_then(|s| s.to_str()) == Some("pem") {
                    let cert_data = fs::read(entry.path())?;
                    self.trusted_clients.write().await.push(cert_data);
                    loaded += 1;
                }
            }
        }

        // Load operational client certs
        if operational_dir.exists() {
            for entry in fs::read_dir(&operational_dir)? {
                let entry = entry?;
                if entry.path().extension().and_then(|s| s.to_str()) == Some("pem") {
                    let cert_data = fs::read(entry.path())?;
                    self.trusted_clients.write().await.push(cert_data);
                    loaded += 1;
                }
            }
        }

        info!("Loaded {} trusted client certificates", loaded);
        Ok(())
    }

    /// Set the current certificate tier mode
    pub async fn set_tier(&self, tier: CertificateTier) {
        *self.tier.write().await = tier;
        info!("Certificate tier set to: {:?}", tier);
    }

    /// Get the current certificate tier
    #[allow(dead_code)]
    pub async fn get_tier(&self) -> CertificateTier {
        *self.tier.read().await
    }

    /// Add a trusted client certificate
    #[allow(dead_code)]
    pub async fn add_trusted_client(&self, cert_data: Vec<u8>) {
        self.trusted_clients.write().await.push(cert_data);
    }

    /// Check if a client certificate is trusted
    #[allow(dead_code)]
    pub async fn is_trusted(&self, cert_data: &[u8]) -> bool {
        let clients = self.trusted_clients.read().await;
        clients.iter().any(|c| c.as_slice() == cert_data)
    }
}

impl Default for TlsManager {
    fn default() -> Self {
        Self::new()
    }
}

/// gRPC interceptor that allows unauthenticated InitBootstrap requests
/// while requiring mTLS for all other endpoints
///
/// Note: This interceptor runs after TLS handshake. For endpoints requiring mTLS,
/// the TLS layer will have already verified the client certificate.
/// This interceptor only serves to allow InitBootstrap through without client cert.
#[allow(clippy::result_large_err)]
pub fn auth_interceptor(request: Request<()>) -> Result<Request<()>, Status> {
    // Check if this is an InitBootstrap request by looking at metadata
    // Since we can't access URI directly, we'll allow all requests through
    // and rely on TLS layer to enforce client certificates
    // InitBootstrap will be configured to not require client cert at TLS level

    debug!("Processing authenticated request");
    Ok(request)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_tls_manager_new() {
        let manager = TlsManager::new();
        assert_eq!(manager.get_tier().await, CertificateTier::Bootstrap);
    }

    #[tokio::test]
    async fn test_set_tier() {
        let manager = TlsManager::new();
        manager.set_tier(CertificateTier::Operational).await;
        assert_eq!(manager.get_tier().await, CertificateTier::Operational);
    }

    #[tokio::test]
    async fn test_add_trusted_client() {
        let manager = TlsManager::new();
        let cert_data = vec![1, 2, 3, 4];
        manager.add_trusted_client(cert_data.clone()).await;
        assert!(manager.is_trusted(&cert_data).await);
    }

    #[tokio::test]
    async fn test_is_not_trusted() {
        let manager = TlsManager::new();
        let cert_data = vec![1, 2, 3, 4];
        assert!(!manager.is_trusted(&cert_data).await);
    }
}
