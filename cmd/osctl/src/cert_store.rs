use std::fs;
use std::path::PathBuf;

/// Certificate storage paths for osctl client
pub struct CertStore {
    base_dir: PathBuf,
}

impl CertStore {
    /// Creates new cert store in ~/.keel/certs
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE")) // Windows fallback
            .map_err(|_| "Could not determine home directory")?;
        let base_dir = PathBuf::from(home).join(".keel/certs");
        fs::create_dir_all(&base_dir)?;
        Ok(Self { base_dir })
    }

    /// Get paths for node-specific certificates
    pub fn get_node_paths(&self, node_ip: &str, tier: &str) -> CertPaths {
        let node_dir = self.base_dir.join(node_ip).join(tier);
        CertPaths {
            dir: node_dir.clone(),
            cert: node_dir.join("client.pem"),
            key: node_dir.join("client.key"),
            ca: node_dir.join("ca.pem"),
        }
    }

    /// Save certificate files
    pub fn save_certs(
        &self,
        node_ip: &str,
        tier: &str,
        cert_pem: &str,
        key_pem: &str,
        ca_pem: Option<&str>,
    ) -> Result<CertPaths, Box<dyn std::error::Error>> {
        let paths = self.get_node_paths(node_ip, tier);
        fs::create_dir_all(&paths.dir)?;

        fs::write(&paths.cert, cert_pem)?;
        fs::write(&paths.key, key_pem)?;

        // Set strict permissions on private key
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&paths.key, fs::Permissions::from_mode(0o600))?;
        }

        if let Some(ca) = ca_pem {
            fs::write(&paths.ca, ca)?;
        }

        Ok(paths)
    }

    /// Load certificates for a node
    pub fn load_certs(
        &self,
        node_ip: &str,
        tier: &str,
    ) -> Result<LoadedCerts, Box<dyn std::error::Error>> {
        let paths = self.get_node_paths(node_ip, tier);
        Ok(LoadedCerts {
            cert_pem: fs::read_to_string(&paths.cert)?,
            key_pem: fs::read_to_string(&paths.key)?,
            ca_pem: if paths.ca.exists() {
                Some(fs::read_to_string(&paths.ca)?)
            } else {
                None
            },
        })
    }

    /// Check if certificates exist for a node and tier
    pub fn certs_exist(&self, node_ip: &str, tier: &str) -> bool {
        let paths = self.get_node_paths(node_ip, tier);
        paths.cert.exists() && paths.key.exists()
    }
}

impl Default for CertStore {
    fn default() -> Self {
        Self::new().expect("Failed to create cert store")
    }
}

#[derive(Debug)]
pub struct CertPaths {
    pub dir: PathBuf,
    pub cert: PathBuf,
    pub key: PathBuf,
    pub ca: PathBuf,
}

#[derive(Debug)]
pub struct LoadedCerts {
    pub cert_pem: String,
    pub key_pem: String,
    pub ca_pem: Option<String>,
}

/// Extract node IP/hostname from endpoint URL
pub fn extract_node_from_endpoint(endpoint: &str) -> Result<String, Box<dyn std::error::Error>> {
    // Parse endpoint like "http://[::1]:50051" or "http://192.168.1.1:50051"
    let url = url::Url::parse(endpoint)?;
    let host = url.host_str().ok_or("No host in endpoint")?.to_string();

    // Remove IPv6 brackets if present
    let host = host.trim_start_matches('[').trim_end_matches(']');

    Ok(host.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_node_ipv4() {
        let result = extract_node_from_endpoint("http://192.168.1.1:50051").unwrap();
        assert_eq!(result, "192.168.1.1");
    }

    #[test]
    fn test_extract_node_ipv6() {
        let result = extract_node_from_endpoint("http://[::1]:50051").unwrap();
        assert_eq!(result, "::1");
    }

    #[test]
    fn test_extract_node_hostname() {
        let result = extract_node_from_endpoint("http://localhost:50051").unwrap();
        assert_eq!(result, "localhost");
    }
}
