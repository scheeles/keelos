//! Network configuration types and validation
//!
//! This module provides types for managing network configuration in KeelOS.
//! Network configuration is persisted to `/var/lib/keel/network/config.json`
//! and applied during boot by `keel-init`.

use ipnetwork::{Ipv4Network, Ipv6Network};
use serde::{Deserialize, Serialize};
use std::fs;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::path::Path;
use thiserror::Error;

const CONFIG_PATH: &str = "/var/lib/keel/network/config.json";

#[derive(Debug, Error)]
pub enum NetworkConfigError {
    #[error("Invalid IP address: {0}")]
    InvalidIpAddress(String),

    #[error("Invalid CIDR notation: {0}")]
    InvalidCidr(String),

    #[error("Invalid interface name: {0}")]
    InvalidInterfaceName(String),

    #[error("Invalid VLAN ID: {0} (must be 1-4094)")]
    InvalidVlanId(u32),

    #[error("Invalid bonding mode: {0}")]
    InvalidBondingMode(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Configuration validation error: {0}")]
    Validation(String),
}

/// Complete network configuration for a node
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NetworkConfig {
    /// Network interface configurations
    #[serde(default)]
    pub interfaces: Vec<InterfaceConfig>,

    /// DNS configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dns: Option<DnsConfig>,

    /// Custom routes
    #[serde(default)]
    pub routes: Vec<RouteConfig>,
}

/// Network interface configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InterfaceConfig {
    /// Interface name (e.g., "eth0", "bond0", "eth0.100")
    pub name: String,

    /// Interface configuration type
    #[serde(flatten)]
    pub config: InterfaceType,
}

/// Types of network interface configurations
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum InterfaceType {
    Dhcp,
    Static(StaticConfig),
    Vlan(VlanConfig),
    Bond(BondConfig),
}

/// Static IP configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StaticConfig {
    /// IPv4 address with CIDR notation (e.g., "192.168.1.100/24")
    pub ipv4_address: String,

    /// Gateway IP address
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gateway: Option<String>,

    /// MTU (default: 1500)
    #[serde(default = "default_mtu")]
    pub mtu: u32,
}

fn default_mtu() -> u32 {
    1500
}

/// VLAN (802.1Q) configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VlanConfig {
    /// Parent interface name
    pub parent: String,

    /// VLAN ID (1-4094)
    pub vlan_id: u32,

    /// IP configuration (DHCP or Static)
    #[serde(flatten)]
    pub ip_config: VlanIpConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "ip_type", rename_all = "lowercase")]
pub enum VlanIpConfig {
    Dhcp,
    Static(StaticConfig),
}

/// Bonding configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BondConfig {
    /// Bonding mode
    pub mode: BondingMode,

    /// Slave interfaces
    pub slaves: Vec<String>,

    /// IP configuration (DHCP or Static)
    #[serde(flatten)]
    pub ip_config: BondIpConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "ip_type", rename_all = "lowercase")]
pub enum BondIpConfig {
    Dhcp,
    Static(StaticConfig),
}

/// Bonding modes
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum BondingMode {
    /// Link Aggregation Control Protocol (802.3ad)
    #[serde(rename = "802.3ad")]
    Lacp,
    /// Active-backup for fault tolerance
    ActiveBackup,
    /// Round-robin load balancing
    BalanceRr,
}

impl BondingMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            BondingMode::Lacp => "802.3ad",
            BondingMode::ActiveBackup => "active-backup",
            BondingMode::BalanceRr => "balance-rr",
        }
    }
}

impl std::str::FromStr for BondingMode {
    type Err = NetworkConfigError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "802.3ad" => Ok(BondingMode::Lacp),
            "active-backup" => Ok(BondingMode::ActiveBackup),
            "balance-rr" => Ok(BondingMode::BalanceRr),
            _ => Err(NetworkConfigError::InvalidBondingMode(s.to_string())),
        }
    }
}

/// DNS configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DnsConfig {
    /// DNS nameserver addresses
    pub nameservers: Vec<String>,

    /// DNS search domains
    #[serde(default)]
    pub search_domains: Vec<String>,
}

/// Static route configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RouteConfig {
    /// Destination network in CIDR notation
    pub destination: String,

    /// Gateway IP address
    pub gateway: String,

    /// Optional metric/priority
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metric: Option<u32>,
}

impl NetworkConfig {
    /// Create a new empty network configuration
    pub fn new() -> Self {
        NetworkConfig {
            interfaces: Vec::new(),
            dns: None,
            routes: Vec::new(),
        }
    }

    /// Load network configuration from the default path
    pub fn load() -> Result<Self, NetworkConfigError> {
        Self::load_from(CONFIG_PATH)
    }

    /// Load network configuration from a specific path
    pub fn load_from<P: AsRef<Path>>(path: P) -> Result<Self, NetworkConfigError> {
        let content = fs::read_to_string(path)?;
        let config: NetworkConfig = serde_json::from_str(&content)?;
        config.validate()?;
        Ok(config)
    }

    /// Save network configuration to the default path
    pub fn save(&self) -> Result<(), NetworkConfigError> {
        self.save_to(CONFIG_PATH)
    }

    /// Save network configuration to a specific path
    pub fn save_to<P: AsRef<Path>>(&self, path: P) -> Result<(), NetworkConfigError> {
        self.validate()?;

        // Ensure parent directory exists
        if let Some(parent) = path.as_ref().parent() {
            fs::create_dir_all(parent)?;
        }

        let content = serde_json::to_string_pretty(self)?;
        fs::write(path, content)?;
        Ok(())
    }

    /// Validate the entire network configuration
    pub fn validate(&self) -> Result<(), NetworkConfigError> {
        // Validate all interfaces
        for iface in &self.interfaces {
            iface.validate()?;
        }

        // Validate DNS configuration
        if let Some(ref dns) = self.dns {
            dns.validate()?;
        }

        // Validate routes
        for route in &self.routes {
            route.validate()?;
        }

        // Check for duplicate interface names
        let mut names = std::collections::HashSet::new();
        for iface in &self.interfaces {
            if !names.insert(&iface.name) {
                return Err(NetworkConfigError::Validation(format!(
                    "Duplicate interface name: {}",
                    iface.name
                )));
            }
        }

        Ok(())
    }
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl InterfaceConfig {
    /// Validate interface configuration
    fn validate(&self) -> Result<(), NetworkConfigError> {
        // Validate interface name (basic check)
        if self.name.is_empty() || self.name.len() > 15 {
            return Err(NetworkConfigError::InvalidInterfaceName(self.name.clone()));
        }

        // Validate per interface type
        match &self.config {
            InterfaceType::Dhcp => Ok(()),
            InterfaceType::Static(cfg) => cfg.validate(),
            InterfaceType::Vlan(cfg) => cfg.validate(),
            InterfaceType::Bond(cfg) => cfg.validate(),
        }
    }
}

impl StaticConfig {
    /// Validate static IP configuration
    fn validate(&self) -> Result<(), NetworkConfigError> {
        // Validate IPv4 address with CIDR
        self.ipv4_address
            .parse::<Ipv4Network>()
            .map_err(|_| NetworkConfigError::InvalidCidr(self.ipv4_address.clone()))?;

        // Validate gateway if present
        if let Some(ref gw) = self.gateway {
            gw.parse::<Ipv4Addr>()
                .map_err(|_| NetworkConfigError::InvalidIpAddress(gw.clone()))?;
        }

        // Validate MTU range
        if self.mtu < 68 || self.mtu > 9000 {
            return Err(NetworkConfigError::Validation(format!(
                "Invalid MTU: {} (must be 68-9000)",
                self.mtu
            )));
        }

        Ok(())
    }
}

impl VlanConfig {
    /// Validate VLAN configuration
    fn validate(&self) -> Result<(), NetworkConfigError> {
        // Validate VLAN ID range
        if self.vlan_id < 1 || self.vlan_id > 4094 {
            return Err(NetworkConfigError::InvalidVlanId(self.vlan_id));
        }

        // Validate parent interface name
        if self.parent.is_empty() {
            return Err(NetworkConfigError::InvalidInterfaceName(
                self.parent.clone(),
            ));
        }

        // Validate IP configuration
        match &self.ip_config {
            VlanIpConfig::Dhcp => Ok(()),
            VlanIpConfig::Static(cfg) => cfg.validate(),
        }
    }
}

impl BondConfig {
    /// Validate bonding configuration
    fn validate(&self) -> Result<(), NetworkConfigError> {
        // Validate slaves
        if self.slaves.is_empty() {
            return Err(NetworkConfigError::Validation(
                "Bond must have at least one slave interface".to_string(),
            ));
        }

        // Validate IP configuration
        match &self.ip_config {
            BondIpConfig::Dhcp => Ok(()),
            BondIpConfig::Static(cfg) => cfg.validate(),
        }
    }
}

impl DnsConfig {
    /// Validate DNS configuration
    fn validate(&self) -> Result<(), NetworkConfigError> {
        if self.nameservers.is_empty() {
            return Err(NetworkConfigError::Validation(
                "DNS configuration must have at least one nameserver".to_string(),
            ));
        }

        // Validate each nameserver is a valid IP
        for ns in &self.nameservers {
            ns.parse::<Ipv4Addr>()
                .or_else(|_| ns.parse::<Ipv6Addr>().map(|_| Ipv4Addr::new(0, 0, 0, 0)))
                .map_err(|_| NetworkConfigError::InvalidIpAddress(ns.clone()))?;
        }

        Ok(())
    }
}

impl RouteConfig {
    /// Validate route configuration
    fn validate(&self) -> Result<(), NetworkConfigError> {
        // Validate destination CIDR
        self.destination
            .parse::<Ipv4Network>()
            .or_else(|_| {
                self.destination
                    .parse::<Ipv6Network>()
                    .map(|_| Ipv4Network::new(Ipv4Addr::new(0, 0, 0, 0), 0).unwrap())
            })
            .map_err(|_| NetworkConfigError::InvalidCidr(self.destination.clone()))?;

        // Validate gateway IP
        self.gateway
            .parse::<Ipv4Addr>()
            .or_else(|_| {
                self.gateway
                    .parse::<Ipv6Addr>()
                    .map(|_| Ipv4Addr::new(0, 0, 0, 0))
            })
            .map_err(|_| NetworkConfigError::InvalidIpAddress(self.gateway.clone()))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_static_config_validation() {
        let valid = StaticConfig {
            ipv4_address: "192.168.1.100/24".to_string(),
            gateway: Some("192.168.1.1".to_string()),
            mtu: 1500,
        };
        assert!(valid.validate().is_ok());

        let invalid_cidr = StaticConfig {
            ipv4_address: "192.168.1.100".to_string(), // Missing /24
            gateway: None,
            mtu: 1500,
        };
        assert!(invalid_cidr.validate().is_err());

        let invalid_gateway = StaticConfig {
            ipv4_address: "192.168.1.100/24".to_string(),
            gateway: Some("not-an-ip".to_string()),
            mtu: 1500,
        };
        assert!(invalid_gateway.validate().is_err());
    }

    #[test]
    fn test_vlan_config_validation() {
        let valid = VlanConfig {
            parent: "eth0".to_string(),
            vlan_id: 100,
            ip_config: VlanIpConfig::Dhcp,
        };
        assert!(valid.validate().is_ok());

        let invalid_vlan_id = VlanConfig {
            parent: "eth0".to_string(),
            vlan_id: 5000, // Out of range
            ip_config: VlanIpConfig::Dhcp,
        };
        assert!(invalid_vlan_id.validate().is_err());
    }

    #[test]
    fn test_dns_config_validation() {
        let valid = DnsConfig {
            nameservers: vec!["8.8.8.8".to_string(), "1.1.1.1".to_string()],
            search_domains: vec!["example.com".to_string()],
        };
        assert!(valid.validate().is_ok());

        let invalid = DnsConfig {
            nameservers: vec!["not-an-ip".to_string()],
            search_domains: vec![],
        };
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn test_network_config_serialization() {
        let config = NetworkConfig {
            interfaces: vec![InterfaceConfig {
                name: "eth0".to_string(),
                config: InterfaceType::Static(StaticConfig {
                    ipv4_address: "10.0.2.100/24".to_string(),
                    gateway: Some("10.0.2.2".to_string()),
                    mtu: 1500,
                }),
            }],
            dns: Some(DnsConfig {
                nameservers: vec!["8.8.8.8".to_string()],
                search_domains: vec![],
            }),
            routes: vec![],
        };

        let json = serde_json::to_string(&config).unwrap();
        let deserialized: NetworkConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(config, deserialized);
    }

    #[test]
    fn test_duplicate_interface_names() {
        let config = NetworkConfig {
            interfaces: vec![
                InterfaceConfig {
                    name: "eth0".to_string(),
                    config: InterfaceType::Dhcp,
                },
                InterfaceConfig {
                    name: "eth0".to_string(),
                    config: InterfaceType::Dhcp,
                },
            ],
            dns: None,
            routes: vec![],
        };

        assert!(config.validate().is_err());
    }
}
