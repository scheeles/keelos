//! Certificate lifecycle manager for KeelOS Agent
//!
//! Monitors certificate expiry and handles automatic rotation.

use keel_crypto::ca::{CaError, CertificateAuthority};
use keel_crypto::rotation::{check_expiry, rotate_certificate, RotationError};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{sleep, Duration};
use tracing::{debug, error, info, warn};

/// Configuration for certificate manager
#[derive(Clone, Debug)]
pub struct CertManagerConfig {
    /// Path to CA certificate
    pub ca_cert_path: PathBuf,
    /// Path to CA private key
    pub ca_key_path: PathBuf,
    /// Path to server certificate
    pub server_cert_path: PathBuf,
    /// Path to server private key
    pub server_key_path: PathBuf,
    /// Days before expiry to trigger rotation
    pub rotation_days_before_expiry: u32,
    /// Certificate validity in days when issuing new certs
    pub cert_validity_days: u32,
    /// Server common name
    pub server_common_name: String,
}

impl Default for CertManagerConfig {
    fn default() -> Self {
        Self {
            ca_cert_path: PathBuf::from("/etc/keel/crypto/ca.pem"),
            ca_key_path: PathBuf::from("/etc/keel/crypto/ca.key"),
            server_cert_path: PathBuf::from("/etc/keel/crypto/server.pem"),
            server_key_path: PathBuf::from("/etc/keel/crypto/server.key"),
            rotation_days_before_expiry: 30,
            cert_validity_days: 90,
            server_common_name: "keel-agent".to_string(),
        }
    }
}

/// Certificate manager service
pub struct CertManager {
    config: CertManagerConfig,
    ca: Arc<RwLock<Option<CertificateAuthority>>>,
    needs_reload: Arc<RwLock<bool>>,
}

impl CertManager {
    /// Create a new certificate manager
    pub fn new(config: CertManagerConfig) -> Self {
        Self {
            config,
            ca: Arc::new(RwLock::new(None)),
            needs_reload: Arc::new(RwLock::new(false)),
        }
    }

    /// Initialize the certificate manager
    /// Loads or generates CA certificate
    pub async fn initialize(&self) -> Result<(), CertManagerError> {
        info!("Initializing certificate manager");

        // Load or generate CA
        let ca = if self.config.ca_cert_path.exists() {
            info!("Loading existing CA certificate");
            CertificateAuthority::load_from_pem(
                &self.config.ca_cert_path,
                &self.config.ca_key_path,
            )?
        } else {
            warn!("CA certificate not found, generating new CA");
            let ca = CertificateAuthority::generate_root_ca(
                "KeelOS CA",
                self.config.cert_validity_days * 10, // CA valid for 10x cert validity
            )?;

            // Save CA to disk  
            if let Some(parent) = self.config.ca_cert_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            ca.save_to_pem(&self.config.ca_cert_path, &self.config.ca_key_path)?;

            info!("Generated and saved new CA certificate");
            ca
        };

        // Store CA
        *self.ca.write().await = Some(ca);

        // Generate initial server cert if it doesn't exist
        if !self.config.server_cert_path.exists() {
            info!("Server certificate not found, generating initial certificate");
            self.rotate_server_certificate().await?;
        }

        Ok(())
    }

    /// Start the certificate rotation monitor
    pub async fn start_rotation_monitor(self: Arc<Self>) {
        info!("Starting certificate rotation monitor");

        loop {
            // Check if rotation is needed
            match self.check_and_rotate().await {
                Ok(should_reload) => {
                    if should_reload {
                        info!("Certificate rotated, signaling reload");
                        *self.needs_reload.write().await = true;
                    }
                }
                Err(e) => {
                    error!(error = %e, "Certificate rotation check failed");
                }
            }

            // Sleep for 1 day before next check
            sleep(Duration::from_secs(86400)).await;
        }
    }

    /// Check if certificate needs rotation and perform it if necessary
    async fn check_and_rotate(&self) -> Result<bool, CertManagerError> {
        debug!("Checking certificate expiry");

        // Load current server certificate
        let cert_pem = std::fs::read_to_string(&self.config.server_cert_path)?;

        // Check expiry
        let expiry = check_expiry(&cert_pem, self.config.rotation_days_before_expiry)?;

        debug!(
            days_until_expiry = expiry.days_until_expiry,
            is_expiring_soon = expiry.is_expiring_soon,
            "Certificate expiry status"
        );

        if expiry.is_expiring_soon {
            info!(
                days_until_expiry = expiry.days_until_expiry,
                "Certificate expiring soon, rotating"
            );
            self.rotate_server_certificate().await?;
            Ok(true)
        } else {
            debug!("Certificate does not need rotation yet");
            Ok(false)
        }
    }

    /// Rotate the server certificate
    pub async fn rotate_server_certificate(&self) -> Result<(), CertManagerError> {
        info!("Rotating server certificate");

        // Get CA
        let ca_guard = self.ca.read().await;
        let ca = ca_guard
            .as_ref()
            .ok_or_else(|| CertManagerError::NotInitialized)?;

        // Issue new certificate
        let (cert_pem, key_pem) = ca.issue_certificate(
            &self.config.server_common_name,
            self.config.cert_validity_days,
            true, // is_server
        )?;

        // Atomically rotate the certificate files
        rotate_certificate(
            &self.config.server_cert_path,
            &self.config.server_key_path,
            &cert_pem,
            &key_pem,
        )?;

        info!("Server certificate rotated successfully");

        Ok(())
    }

    /// Check if a reload is needed (certificate was rotated)
    pub async fn needs_reload(&self) -> bool {
        *self.needs_reload.read().await
    }

    /// Clear the reload flag
    pub async fn clear_reload_flag(&self) {
        *self.needs_reload.write().await = false;
    }

    /// Get CA certificate PEM for distribution to clients
    pub async fn get_ca_cert_pem(&self) -> Result<String, CertManagerError> {
        let ca_guard = self.ca.read().await;
        let ca = ca_guard
            .as_ref()
            .ok_or_else(|| CertManagerError::NotInitialized)?;

        Ok(ca.ca_cert_pem().to_string())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CertManagerError {
    #[error("Certificate manager not initialized")]
    NotInitialized,
    #[error("CA error: {0}")]
    Ca(#[from] CaError),
    #[error("Rotation error: {0}")]
    Rotation(#[from] RotationError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_cert_manager_initialize() {
        let temp_dir = TempDir::new().unwrap();

        let config = CertManagerConfig {
            ca_cert_path: temp_dir.path().join("ca.pem"),
            ca_key_path: temp_dir.path().join("ca.key"),
            server_cert_path: temp_dir.path().join("server.pem"),
            server_key_path: temp_dir.path().join("server.key"),
            rotation_days_before_expiry: 30,
            cert_validity_days: 90,
            server_common_name: "test-agent".to_string(),
        };

        let manager = CertManager::new(config);
        manager.initialize().await.unwrap();

        // Verify files were created
        assert!(temp_dir.path().join("ca.pem").exists());
        assert!(temp_dir.path().join("ca.key").exists());
        assert!(temp_dir.path().join("server.pem").exists());
        assert!(temp_dir.path().join("server.key").exists());
    }

    #[tokio::test]
    async fn test_cert_rotation() {
        let temp_dir = TempDir::new().unwrap();

        let config = CertManagerConfig {
            ca_cert_path: temp_dir.path().join("ca.pem"),
            ca_key_path: temp_dir.path().join("ca.key"),
            server_cert_path: temp_dir.path().join("server.pem"),
            server_key_path: temp_dir.path().join("server.key"),
            rotation_days_before_expiry: 30,
            cert_validity_days: 10, // Short validity for testing
            server_common_name: "test-agent".to_string(),
        };

        let manager = CertManager::new(config);
        manager.initialize().await.unwrap();

        // Read initial cert
        let old_cert = std::fs::read_to_string(temp_dir.path().join("server.pem")).unwrap();

        // Force rotation
        manager.rotate_server_certificate().await.unwrap();

        // Read rotated cert
        let new_cert = std::fs::read_to_string(temp_dir.path().join("server.pem")).unwrap();

        // Verify it changed
        assert_ne!(old_cert, new_cert);
    }
}
