//! Certificate storage module for osctl
//!
//! Manages local certificate storage in ~/.keel/certs/<node>/<tier>/

use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct CertificatePaths {
    pub cert: PathBuf,
    pub key: PathBuf,
    pub ca: Option<PathBuf>,
}

pub struct CertStore {
    base_dir: PathBuf,
}

impl CertStore {
    /// Create a new cert store using ~/.keel/certs
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let home = dirs::home_dir()
            .ok_or("Could not determine home directory")?;
        let base_dir = home.join(".keel").join("certs");
        fs::create_dir_all(&base_dir)?;
        
        Ok(Self { base_dir })
    }

    /// Save certificates for a specific node and tier
    /// tier can be "bootstrap" or "operational"
    pub fn save_certs(
        &self,
        node_id: &str,
        tier: &str,
        cert_pem: &str,
        key_pem: &str,
        ca_pem: Option<&str>,
    ) -> Result<CertificatePaths, Box<dyn std::error::Error>> {
        // Create directory: ~/.keel/certs/<node>/<tier>/
        let cert_dir = self.base_dir.join(node_id).join(tier);
        fs::create_dir_all(&cert_dir)?;

        // Save cert and key
        let cert_path = cert_dir.join("client.pem");
        let key_path = cert_dir.join("client.key");

        fs::write(&cert_path, cert_pem)?;
        fs::write(&key_path, key_pem)?;

        // Set restrictive permissions on private key
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&key_path)?.permissions();
            perms.set_mode(0o600);
            fs::set_permissions(&key_path, perms)?;
        }

        // Save CA if provided
        let ca_path = if let Some(ca) = ca_pem {
            let path = cert_dir.join("ca.pem");
            fs::write(&path, ca)?;
            Some(path)
        } else {
            None
        };

        Ok(CertificatePaths {
            cert: cert_path,
            key: key_path,
            ca: ca_path,
        })
    }

    /// Load certificates for a specific node and tier
    pub fn load_certs(
        &self,
        node_id: &str,
        tier: &str,
    ) -> Result<CertificatePaths, Box<dyn std::error::Error>> {
        let cert_dir = self.base_dir.join(node_id).join(tier);
        
        if !cert_dir.exists() {
            return Err(format!("No certificates found for node {} tier {}", node_id, tier).into());
        }

        let cert_path = cert_dir.join("client.pem");
        let key_path = cert_dir.join("client.key");
        let ca_path = cert_dir.join("ca.pem");

        if !cert_path.exists() || !key_path.exists() {
            return Err("Certificate files not found".into());
        }

        Ok(CertificatePaths {
            cert: cert_path,
            key: key_path,
            ca: if ca_path.exists() { Some(ca_path) } else { None },
        })
    }

    /// Find best available certificate for a node (operational first, then bootstrap)
    pub fn find_best_cert(&self, node_id: &str) -> Result<(String, CertificatePaths), Box<dyn std::error::Error>> {
        // Try operational first
        if let Ok(paths) = self.load_certs(node_id, "operational") {
            return Ok(("operational".to_string(), paths));
        }

        // Fall back to bootstrap
        if let Ok(paths) = self.load_certs(node_id, "bootstrap") {
            return Ok(("bootstrap".to_string(), paths));
        }

        Err(format!("No certificates found for node {}", node_id).into())
    }
}

/// Extract node ID from endpoint (e.g., "http://192.168.1.10:50051" -> "192_168_1_10")
pub fn extract_node_from_endpoint(endpoint: &str) -> Result<String, Box<dyn std::error::Error>> {
    let url = endpoint.trim_start_matches("http://").trim_start_matches("https://");
    let host = url.split(':').next().ok_or("Invalid endpoint format")?;
    
    // Replace dots and colons with underscores for filesystem safety
    let node_id = host.replace('.', "_").replace(':', "_");
    Ok(node_id)
}
