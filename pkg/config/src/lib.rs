use serde::{Deserialize, Serialize};
use std::fs::File;
use std::path::Path;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("YAML error: {0}")]
    Yaml(#[from] serde_yaml::Error),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NodeConfig {
    pub version: String,
    pub hostname: String,
    #[serde(default)]
    pub kubernetes: KubernetesConfig,
    pub containers: Vec<ContainerConfig>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct KubernetesConfig {
    pub version: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ContainerConfig {
    pub name: String,
    pub image: String,
}

impl NodeConfig {
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self, ConfigError> {
        let file = File::open(path)?;
        let config: NodeConfig = serde_yaml::from_reader(file)?;
        Ok(config)
    }

    pub fn default_config() -> Self {
        Self {
            version: "v1".to_string(),
            hostname: "matic-node".to_string(),
            kubernetes: KubernetesConfig::default(),
            containers: vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_load_config() {
        let yaml = r#"
version: v1
hostname: matic-01
containers:
  - name: test-net
    image: alpine:latest
"#;
        let mut file = NamedTempFile::new().unwrap();
        write!(file, "{}", yaml).unwrap();

        let config = NodeConfig::load(file.path()).unwrap();
        assert_eq!(config.hostname, "matic-01");
        assert_eq!(config.containers.len(), 1);
        assert_eq!(config.containers[0].name, "test-net");
    }

    #[test]
    fn test_default_config() {
        let config = NodeConfig::default_config();
        assert_eq!(config.version, "v1");
        assert_eq!(config.hostname, "matic-node");
        assert!(config.containers.is_empty());
        assert!(config.kubernetes.version.is_none());
    }

    #[test]
    fn test_config_with_kubernetes() {
        let yaml = r#"
version: v1
hostname: k8s-node
kubernetes:
  version: "1.29.0"
containers: []
"#;
        let mut file = NamedTempFile::new().unwrap();
        write!(file, "{}", yaml).unwrap();

        let config = NodeConfig::load(file.path()).unwrap();
        assert_eq!(config.hostname, "k8s-node");
        assert_eq!(config.kubernetes.version, Some("1.29.0".to_string()));
    }

    #[test]
    fn test_config_serialization() {
        let config = NodeConfig {
            version: "v1".to_string(),
            hostname: "test-node".to_string(),
            kubernetes: KubernetesConfig {
                version: Some("1.28.0".to_string()),
            },
            containers: vec![
                ContainerConfig {
                    name: "nginx".to_string(),
                    image: "nginx:latest".to_string(),
                },
            ],
        };

        let yaml = serde_yaml::to_string(&config).unwrap();
        assert!(yaml.contains("hostname: test-node"));
        assert!(yaml.contains("nginx:latest"));
    }

    #[test]
    fn test_load_nonexistent_file() {
        let result = NodeConfig::load("/nonexistent/path/config.yaml");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, ConfigError::Io(_)));
    }

    #[test]
    fn test_load_invalid_yaml() {
        let invalid_yaml = "not: valid: yaml: [";
        let mut file = NamedTempFile::new().unwrap();
        write!(file, "{}", invalid_yaml).unwrap();

        let result = NodeConfig::load(file.path());
        assert!(result.is_err());
    }
}
