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
    pub containers: Vec<ContainerConfig>,
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
}
