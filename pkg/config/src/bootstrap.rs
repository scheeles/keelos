// Kubernetes bootstrap configuration management
//
// This module handles:
// - Bootstrap state persistence
// - Kubeconfig generation and validation
// - CA certificate management

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum BootstrapError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),
    #[error("Missing required field: {0}")]
    MissingField(String),
}

/// Bootstrap configuration persisted to disk
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BootstrapConfig {
    /// Kubernetes API server endpoint
    pub api_server: String,
    /// Node name in the cluster
    pub node_name: String,
    /// Path to kubeconfig file
    pub kubeconfig_path: String,
    /// Path to CA certificate
    pub ca_cert_path: String,
    /// Timestamp when bootstrap was performed (RFC3339)
    pub bootstrapped_at: String,
}

impl BootstrapConfig {
    /// Create a new bootstrap configuration
    pub fn new(
        api_server: String,
        node_name: String,
        kubeconfig_path: String,
        ca_cert_path: String,
    ) -> Self {
        Self {
            api_server,
            node_name,
            kubeconfig_path,
            ca_cert_path,
            bootstrapped_at: chrono::Utc::now().to_rfc3339(),
        }
    }

    /// Save configuration to a JSON file
    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<(), BootstrapError> {
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)?;
        Ok(())
    }

    /// Load configuration from a JSON file
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self, BootstrapError> {
        let contents = fs::read_to_string(path)?;
        let config: BootstrapConfig = serde_json::from_str(&contents)?;
        Ok(config)
    }

    /// Check if the node is bootstrapped by checking if config file exists
    pub fn is_bootstrapped<P: AsRef<Path>>(path: P) -> bool {
        path.as_ref().exists()
    }
}

/// Generate a kubeconfig file for kubelet
///
/// # Arguments
/// * `api_server` - Kubernetes API server endpoint (e.g., "https://k8s.example.com:6443")
/// * `ca_cert_pem` - Cluster CA certificate in PEM format
/// * `bootstrap_token` - Bootstrap token in format "<token-id>.<token-secret>"
/// * `node_name` - Name of the node in the cluster
///
/// # Returns
/// A kubeconfig YAML string ready to be written to disk
pub fn generate_kubeconfig(
    api_server: &str,
    ca_cert_pem: &str,
    bootstrap_token: &str,
    _node_name: &str,
) -> Result<String, BootstrapError> {
    // Validate inputs
    if api_server.is_empty() {
        return Err(BootstrapError::MissingField("api_server".to_string()));
    }
    if ca_cert_pem.is_empty() {
        return Err(BootstrapError::MissingField("ca_cert_pem".to_string()));
    }
    if bootstrap_token.is_empty() {
        return Err(BootstrapError::MissingField("bootstrap_token".to_string()));
    }

    // Validate bootstrap token format (should be <token-id>.<token-secret>)
    if !bootstrap_token.contains('.') || bootstrap_token.split('.').count() != 2 {
        return Err(BootstrapError::InvalidConfig(
            "bootstrap_token must be in format <token-id>.<token-secret>".to_string(),
        ));
    }

    // Base64 encode the CA certificate
    use base64::{engine::general_purpose, Engine as _};
    let ca_data = general_purpose::STANDARD.encode(ca_cert_pem);

    // Generate kubeconfig YAML
    let kubeconfig = format!(
        r#"apiVersion: v1
kind: Config
clusters:
- cluster:
    certificate-authority-data: {ca_data}
    server: {api_server}
  name: kubernetes
contexts:
- context:
    cluster: kubernetes
    user: kubelet
  name: kubelet@kubernetes
current-context: kubelet@kubernetes
users:
- name: kubelet
  user:
    token: {bootstrap_token}
"#,
        ca_data = ca_data,
        api_server = api_server,
        bootstrap_token = bootstrap_token
    );

    Ok(kubeconfig)
}

/// Prepare the Kubernetes directory structure
///
/// Creates the necessary directories for bootstrap configuration:
/// - `/var/lib/keel/kubernetes/` - Main directory for K8s config
///
/// # Arguments
/// * `base_path` - Base path (typically "/var/lib/keel")
pub fn prepare_k8s_directories<P: AsRef<Path>>(base_path: P) -> Result<(), BootstrapError> {
    let k8s_dir = base_path.as_ref().join("kubernetes");
    fs::create_dir_all(&k8s_dir)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::{NamedTempFile, TempDir};

    #[test]
    fn test_bootstrap_config_new() {
        let config = BootstrapConfig::new(
            "https://k8s.example.com:6443".to_string(),
            "node-01".to_string(),
            "/var/lib/keel/kubernetes/kubelet.kubeconfig".to_string(),
            "/var/lib/keel/kubernetes/ca.crt".to_string(),
        );

        assert_eq!(config.api_server, "https://k8s.example.com:6443");
        assert_eq!(config.node_name, "node-01");
        assert!(!config.bootstrapped_at.is_empty());
    }

    #[test]
    fn test_bootstrap_config_save_load() {
        let config = BootstrapConfig::new(
            "https://k8s.example.com:6443".to_string(),
            "node-01".to_string(),
            "/var/lib/keel/kubernetes/kubelet.kubeconfig".to_string(),
            "/var/lib/keel/kubernetes/ca.crt".to_string(),
        );

        let file = NamedTempFile::new().unwrap();
        let path = file.path().to_path_buf();

        config.save(&path).unwrap();
        let loaded = BootstrapConfig::load(&path).unwrap();

        assert_eq!(loaded.api_server, config.api_server);
        assert_eq!(loaded.node_name, config.node_name);
        assert_eq!(loaded.bootstrapped_at, config.bootstrapped_at);
    }

    #[test]
    fn test_is_bootstrapped() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("bootstrap.json");

        assert!(!BootstrapConfig::is_bootstrapped(&config_path));

        let config = BootstrapConfig::new(
            "https://k8s.example.com:6443".to_string(),
            "node-01".to_string(),
            "/var/lib/keel/kubernetes/kubelet.kubeconfig".to_string(),
            "/var/lib/keel/kubernetes/ca.crt".to_string(),
        );
        config.save(&config_path).unwrap();

        assert!(BootstrapConfig::is_bootstrapped(&config_path));
    }

    #[test]
    fn test_generate_kubeconfig() {
        let ca_cert = "-----BEGIN CERTIFICATE-----\nMIIC5zCCAc+gAwIBAgIBADANBgkqhkiG9w0BAQsFADAVMRMwEQYDVQQDEwprdWJl\n-----END CERTIFICATE-----\n";
        let token = "abcdef.0123456789abcdef";

        let kubeconfig =
            generate_kubeconfig("https://k8s.example.com:6443", ca_cert, token, "node-01").unwrap();

        assert!(kubeconfig.contains("apiVersion: v1"));
        assert!(kubeconfig.contains("kind: Config"));
        assert!(kubeconfig.contains("https://k8s.example.com:6443"));
        assert!(kubeconfig.contains("token: abcdef.0123456789abcdef"));
        assert!(kubeconfig.contains("certificate-authority-data:"));
    }

    #[test]
    fn test_generate_kubeconfig_invalid_token() {
        let ca_cert = "-----BEGIN CERTIFICATE-----\ntest\n-----END CERTIFICATE-----\n";
        let result = generate_kubeconfig(
            "https://k8s.example.com:6443",
            ca_cert,
            "invalid-token-format",
            "node-01",
        );

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            BootstrapError::InvalidConfig(_)
        ));
    }

    #[test]
    fn test_generate_kubeconfig_missing_fields() {
        let result = generate_kubeconfig("", "ca-cert", "abc.def", "node");
        assert!(matches!(result, Err(BootstrapError::MissingField(_))));

        let result = generate_kubeconfig("https://api", "", "abc.def", "node");
        assert!(matches!(result, Err(BootstrapError::MissingField(_))));

        let result = generate_kubeconfig("https://api", "ca-cert", "", "node");
        assert!(matches!(result, Err(BootstrapError::MissingField(_))));
    }

    #[test]
    fn test_prepare_k8s_directories() {
        let temp_dir = TempDir::new().unwrap();
        prepare_k8s_directories(temp_dir.path()).unwrap();

        let k8s_dir = temp_dir.path().join("kubernetes");
        assert!(k8s_dir.exists());
        assert!(k8s_dir.is_dir());
    }
}
