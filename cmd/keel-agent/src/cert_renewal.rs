use crate::k8s_csr::K8sCsrManager;
use keel_crypto::{check_cert_needs_renewal, parse_cert_expiry};
use std::sync::Arc;
use tokio::time::{interval, Duration};
use tracing::{debug, error, info, warn};

/// Configuration for certificate renewal
#[derive(Debug, Clone)]
pub struct CertRenewalConfig {
    pub operational_cert_path: String,
    pub operational_key_path: String,
    pub renewal_threshold_days: u32,
    pub check_interval_hours: u64,
}

impl Default for CertRenewalConfig {
    fn default() -> Self {
        Self {
            operational_cert_path: "/var/lib/keel/crypto/operational.pem".to_string(),
            operational_key_path: "/var/lib/keel/crypto/operational.key".to_string(),
            renewal_threshold_days: 30,
            check_interval_hours: 24,
        }
    }
}

/// Manages automatic certificate renewal in the background
pub struct CertRenewalManager {
    cert_path: String,
    key_path: String,
    threshold_days: u32,
    check_interval: Duration,
}

impl CertRenewalManager {
    pub fn new(config: CertRenewalConfig) -> Self {
        Self {
            cert_path: config.operational_cert_path,
            key_path: config.operational_key_path,
            threshold_days: config.renewal_threshold_days,
            check_interval: Duration::from_secs(config.check_interval_hours * 3600),
        }
    }

    /// Start the background renewal loop
    /// This task runs indefinitely, checking and renewing certificates as needed
    pub async fn start_renewal_loop(self: Arc<Self>) {
        info!(
            "Starting certificate auto-renewal (checking every {} hours, renewal threshold {} days)",
            self.check_interval.as_secs() / 3600,
            self.threshold_days
        );

        let mut ticker = interval(self.check_interval);

        loop {
            ticker.tick().await;

            if let Err(e) = self.check_and_renew().await {
                error!("Certificate renewal check failed: {}", e);
            }
        }
    }

    /// Check if renewal is needed and trigger if necessary
    async fn check_and_renew(&self) -> Result<(), String> {
        // Check if cert exists
        if !std::path::Path::new(&self.cert_path).exists() {
            debug!(
                "No operational certificate found at {}, skipping renewal check",
                self.cert_path
            );
            return Ok(());
        }

        // Check if renewal is needed
        let needs_renewal = check_cert_needs_renewal(&self.cert_path, self.threshold_days)?;

        if needs_renewal {
            // Get current expiry for logging
            let cert_pem = std::fs::read_to_string(&self.cert_path)
                .map_err(|e| format!("Failed to read cert: {}", e))?;
            let expiry = parse_cert_expiry(&cert_pem)?;

            info!(
                "Certificate expiring soon ({}), triggering automatic renewal...",
                expiry.format("%Y-%m-%d %H:%M:%S UTC")
            );

            self.trigger_renewal().await?;
        } else {
            debug!("Certificate is valid, no renewal needed");
        }

        Ok(())
    }

    /// Trigger actual certificate renewal
    async fn trigger_renewal(&self) -> Result<(), String> {
        // Check if running in K8s
        if !std::path::Path::new("/var/run/secrets/kubernetes.io/serviceaccount/token").exists() {
            return Err(
                "Not running in Kubernetes - auto-renewal only available in K8s clusters"
                    .to_string(),
            );
        }

        // Get node name
        let node_name = std::env::var("NODE_NAME")
            .ok()
            .or_else(|| hostname::get().ok().and_then(|h| h.into_string().ok()))
            .ok_or_else(|| "Failed to determine node name".to_string())?;

        info!("Auto-renewing certificate for node: {}", node_name);

        // Create CSR manager and request new certificate
        let csr_manager = K8sCsrManager::new(node_name)
            .await
            .map_err(|e| format!("Failed to initialize CSR manager: {}", e))?;

        let (cert_pem, key_pem) = csr_manager
            .request_certificate()
            .await
            .map_err(|e| format!("Failed to request certificate: {}", e))?;

        // Create backup of old certificates
        if std::path::Path::new(&self.cert_path).exists() {
            let backup_cert = format!("{}.backup", self.cert_path);
            let backup_key = format!("{}.backup", self.key_path);

            if let Err(e) = std::fs::copy(&self.cert_path, &backup_cert) {
                warn!("Failed to backup old certificate: {}", e);
            } else {
                info!("Backed up old certificate to {}", backup_cert);
            }

            if let Err(e) = std::fs::copy(&self.key_path, &backup_key) {
                warn!("Failed to backup old key: {}", e);
            } else {
                info!("Backed up old key to {}", backup_key);
            }
        }

        // Write new certificates
        std::fs::write(&self.cert_path, &cert_pem)
            .map_err(|e| format!("Failed to write new certificate: {}", e))?;

        std::fs::write(&self.key_path, &key_pem)
            .map_err(|e| format!("Failed to write new key: {}", e))?;

        // Parse and log new expiry
        if let Ok(new_expiry) = parse_cert_expiry(&cert_pem) {
            info!(
                "✓ Certificate auto-renewal successful! New expiry: {}",
                new_expiry.format("%Y-%m-%d %H:%M:%S UTC")
            );
        } else {
            info!("✓ Certificate auto-renewal successful!");
        }

        Ok(())
    }
}
