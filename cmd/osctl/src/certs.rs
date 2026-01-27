use anyhow::{anyhow, Result};
use std::path::PathBuf;

pub struct CertPaths {
    pub cert: PathBuf,
    pub key: PathBuf,
    pub ca: PathBuf,
}

/// Determine which certificates to use based on the command
/// Bootstrap commands use bootstrap certs, all others use operational certs
pub fn select_certificate_paths(command_name: &str) -> Result<CertPaths> {
    let home_dir = dirs::home_dir().ok_or_else(|| anyhow!("Could not determine home directory"))?;
    let keel_dir = home_dir.join(".keel");

    // Bootstrap commands use bootstrap certificates
    if command_name == "bootstrap" || command_name == "bootstrap-status" {
        let bootstrap_cert = keel_dir.join("bootstrap/client.pem");
        let bootstrap_key = keel_dir.join("bootstrap/client.key");
        let bootstrap_ca = keel_dir.join("bootstrap/ca.pem");

        // Verify bootstrap certificates exist
        if !bootstrap_cert.exists() {
            return Err(anyhow!(
                "Bootstrap certificates not found.\n   Run: osctl init --bootstrap --node <node-ip>"
            ));
        }

        Ok(CertPaths {
            cert: bootstrap_cert,
            key: bootstrap_key,
            ca: bootstrap_ca,
        })
    } else {
        // All other commands use operational (K8s-issued) certificates
        let operational_cert = keel_dir.join("client.pem");
        let operational_key = keel_dir.join("client.key");
        let operational_ca = keel_dir.join("ca.pem");

        // Verify operational certificates exist
        if !operational_cert.exists() {
            return Err(anyhow!(
                "Operational certificates not found.\n   Run: osctl init --kubeconfig ~/.kube/config"
            ));
        }

        Ok(CertPaths {
            cert: operational_cert,
            key: operational_key,
            ca: operational_ca,
        })
    }
}

/// Check if we should use mTLS for this command
/// Returns true if certificates are available
pub fn should_use_mtls(command_name: &str) -> bool {
    select_certificate_paths(command_name).is_ok()
}
