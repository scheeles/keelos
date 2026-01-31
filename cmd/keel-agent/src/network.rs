//! Network management RPC implementations
//!
//! This module provides the implementation for network configuration RPCs.

use keel_api::node::*;
use tonic::{Request, Response, Status};
use tracing::{debug, error, info};

/// Configure network interfaces and DNS
pub async fn configure_network(
    request: Request<ConfigureNetworkRequest>,
) -> Result<Response<ConfigureNetworkResponse>, Status> {
    let req = request.into_inner();

    info!("Network configuration request received");

    // Convert proto messages to keel-config types
    let mut config = keel_config::network::NetworkConfig::new();

    // Convert interfaces
    for proto_iface in req.interfaces {
        let iface_config = match proto_iface.config {
            Some(network_interface::Config::Dhcp(_)) => keel_config::network::InterfaceType::Dhcp,
            Some(network_interface::Config::Static(static_cfg)) => {
                keel_config::network::InterfaceType::Static(keel_config::network::StaticConfig {
                    ipv4_address: static_cfg.ipv4_address,
                    gateway: if static_cfg.gateway.is_empty() {
                        None
                    } else {
                        Some(static_cfg.gateway)
                    },
                    mtu: if static_cfg.mtu == 0 {
                        1500
                    } else {
                        static_cfg.mtu
                    },
                    ipv6_addresses: static_cfg.ipv6_addresses,
                    ipv6_gateway: if static_cfg.ipv6_gateway.is_empty() {
                        None
                    } else {
                        Some(static_cfg.ipv6_gateway)
                    },
                    ipv6_auto: static_cfg.ipv6_auto,
                })
            }
            Some(network_interface::Config::Vlan(vlan_cfg)) => {
                let ip_config = match vlan_cfg.ip_config {
                    Some(vlan_config::IpConfig::Dhcp(_)) => {
                        keel_config::network::VlanIpConfig::Dhcp
                    }
                    Some(vlan_config::IpConfig::Static(s)) => {
                        keel_config::network::VlanIpConfig::Static(
                            keel_config::network::StaticConfig {
                                ipv4_address: s.ipv4_address,
                                gateway: if s.gateway.is_empty() {
                                    None
                                } else {
                                    Some(s.gateway)
                                },
                                mtu: if s.mtu == 0 { 1500 } else { s.mtu },
                                ipv6_addresses: s.ipv6_addresses,
                                ipv6_gateway: if s.ipv6_gateway.is_empty() {
                                    None
                                } else {
                                    Some(s.ipv6_gateway)
                                },
                                ipv6_auto: s.ipv6_auto,
                            },
                        )
                    }
                    None => keel_config::network::VlanIpConfig::Dhcp,
                };

                keel_config::network::InterfaceType::Vlan(keel_config::network::VlanConfig {
                    parent: vlan_cfg.parent,
                    vlan_id: vlan_cfg.vlan_id,
                    ip_config,
                })
            }
            Some(network_interface::Config::Bond(bond_cfg)) => {
                let mode = bond_cfg
                    .mode
                    .parse::<keel_config::network::BondingMode>()
                    .map_err(|e| {
                        Status::invalid_argument(format!("Invalid bonding mode: {}", e))
                    })?;

                let ip_config = match bond_cfg.ip_config {
                    Some(bond_config::IpConfig::Dhcp(_)) => {
                        keel_config::network::BondIpConfig::Dhcp
                    }
                    Some(bond_config::IpConfig::Static(s)) => {
                        keel_config::network::BondIpConfig::Static(
                            keel_config::network::StaticConfig {
                                ipv4_address: s.ipv4_address,
                                gateway: if s.gateway.is_empty() {
                                    None
                                } else {
                                    Some(s.gateway)
                                },
                                mtu: if s.mtu == 0 { 1500 } else { s.mtu },
                                ipv6_addresses: s.ipv6_addresses,
                                ipv6_gateway: if s.ipv6_gateway.is_empty() {
                                    None
                                } else {
                                    Some(s.ipv6_gateway)
                                },
                                ipv6_auto: s.ipv6_auto,
                            },
                        )
                    }
                    None => keel_config::network::BondIpConfig::Dhcp,
                };

                keel_config::network::InterfaceType::Bond(keel_config::network::BondConfig {
                    mode,
                    slaves: bond_cfg.slaves,
                    ip_config,
                })
            }
            None => {
                return Err(Status::invalid_argument("Interface config is required"));
            }
        };

        config
            .interfaces
            .push(keel_config::network::InterfaceConfig {
                name: proto_iface.name,
                config: iface_config,
            });
    }

    // Convert DNS config
    if let Some(dns) = req.dns {
        config.dns = Some(keel_config::network::DnsConfig {
            nameservers: dns.nameservers,
            search_domains: dns.search_domains,
        });
    }

    // Convert routes
    for route in req.routes {
        config.routes.push(keel_config::network::RouteConfig {
            destination: route.destination,
            gateway: route.gateway,
            metric: if route.metric == 0 {
                None
            } else {
                Some(route.metric)
            },
        });
    }

    // Validate configuration
    if let Err(e) = config.validate() {
        return Err(Status::invalid_argument(format!(
            "Invalid network configuration: {}",
            e
        )));
    }

    // Save configuration
    match config.save() {
        Ok(_) => {
            info!("Network configuration saved successfully");

            // Auto-reboot if requested
            if req.auto_reboot {
                info!("Auto-reboot requested, scheduling reboot");
                tokio::spawn(async {
                    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                    let _ = std::process::Command::new("reboot").status();
                });
            }

            Ok(Response::new(ConfigureNetworkResponse {
                success: true,
                message: "Network configuration saved. Changes will apply on next boot."
                    .to_string(),
                reboot_required: true,
            }))
        }
        Err(e) => {
            error!(error = %e, "Failed to save network configuration");
            Err(Status::internal(format!(
                "Failed to save configuration: {}",
                e
            )))
        }
    }
}

/// Get current network configuration
pub async fn get_network_config(
    _request: Request<GetNetworkConfigRequest>,
) -> Result<Response<GetNetworkConfigResponse>, Status> {
    debug!("Get network config requested");

    match keel_config::network::NetworkConfig::load() {
        Ok(config) => {
            // Convert to proto types
            let proto_interfaces: Vec<NetworkInterface> = config
                .interfaces
                .into_iter()
                .map(|iface| {
                    let proto_config = match iface.config {
                        keel_config::network::InterfaceType::Dhcp => {
                            Some(network_interface::Config::Dhcp(DhcpConfig {
                                enabled: true,
                            }))
                        }
                        keel_config::network::InterfaceType::Static(cfg) => {
                            Some(network_interface::Config::Static(StaticConfig {
                                ipv4_address: cfg.ipv4_address,
                                gateway: cfg.gateway.unwrap_or_default(),
                                mtu: cfg.mtu,
                                ipv6_addresses: cfg.ipv6_addresses,
                                ipv6_gateway: cfg.ipv6_gateway.unwrap_or_default(),
                                ipv6_auto: cfg.ipv6_auto,
                            }))
                        }
                        keel_config::network::InterfaceType::Vlan(cfg) => {
                            let ip_config = match cfg.ip_config {
                                keel_config::network::VlanIpConfig::Dhcp => {
                                    Some(vlan_config::IpConfig::Dhcp(DhcpConfig { enabled: true }))
                                }
                                keel_config::network::VlanIpConfig::Static(s) => {
                                    Some(vlan_config::IpConfig::Static(StaticConfig {
                                        ipv4_address: s.ipv4_address,
                                        gateway: s.gateway.unwrap_or_default(),
                                        mtu: s.mtu,
                                        ipv6_addresses: s.ipv6_addresses,
                                        ipv6_gateway: s.ipv6_gateway.unwrap_or_default(),
                                        ipv6_auto: s.ipv6_auto,
                                    }))
                                }
                            };

                            Some(network_interface::Config::Vlan(VlanConfig {
                                parent: cfg.parent,
                                vlan_id: cfg.vlan_id,
                                ip_config,
                            }))
                        }
                        keel_config::network::InterfaceType::Bond(cfg) => {
                            let ip_config = match cfg.ip_config {
                                keel_config::network::BondIpConfig::Dhcp => {
                                    Some(bond_config::IpConfig::Dhcp(DhcpConfig { enabled: true }))
                                }
                                keel_config::network::BondIpConfig::Static(s) => {
                                    Some(bond_config::IpConfig::Static(StaticConfig {
                                        ipv4_address: s.ipv4_address,
                                        gateway: s.gateway.unwrap_or_default(),
                                        mtu: s.mtu,
                                        ipv6_addresses: s.ipv6_addresses,
                                        ipv6_gateway: s.ipv6_gateway.unwrap_or_default(),
                                        ipv6_auto: s.ipv6_auto,
                                    }))
                                }
                            };

                            Some(network_interface::Config::Bond(BondConfig {
                                mode: cfg.mode.as_str().to_string(),
                                slaves: cfg.slaves,
                                ip_config,
                            }))
                        }
                    };

                    NetworkInterface {
                        name: iface.name,
                        config: proto_config,
                    }
                })
                .collect();

            let proto_dns = config.dns.map(|dns| DnsConfig {
                nameservers: dns.nameservers,
                search_domains: dns.search_domains,
            });

            let proto_routes: Vec<NetworkRoute> = config
                .routes
                .into_iter()
                .map(|route| NetworkRoute {
                    destination: route.destination,
                    gateway: route.gateway,
                    metric: route.metric.unwrap_or(0),
                })
                .collect();

            Ok(Response::new(GetNetworkConfigResponse {
                interfaces: proto_interfaces,
                dns: proto_dns,
                routes: proto_routes,
            }))
        }
        Err(e) => {
            debug!(error = %e, "No network configuration found");
            // Return empty configuration
            Ok(Response::new(GetNetworkConfigResponse {
                interfaces: vec![],
                dns: None,
                routes: vec![],
            }))
        }
    }
}

/// Get runtime network status
pub async fn get_network_status(
    _request: Request<GetNetworkStatusRequest>,
) -> Result<Response<GetNetworkStatusResponse>, Status> {
    debug!("Get network status requested");

    let mut interfaces = Vec::new();

    // Read /sys/class/net to get all network interfaces
    if let Ok(entries) = std::fs::read_dir("/sys/class/net") {
        for entry in entries.flatten() {
            if let Some(iface_name) = entry.file_name().to_str() {
                // Skip loopback
                if iface_name == "lo" {
                    continue;
                }

                let mut iface_status = InterfaceStatus {
                    name: iface_name.to_string(),
                    state: "unknown".to_string(),
                    ipv4_addresses: vec![],
                    mac_address: String::new(),
                    mtu: 0,
                    statistics: None,
                    ipv6_addresses: vec![],
                    ipv6_address_info: vec![],
                };

                // Read operstate
                let operstate_path = format!("/sys/class/net/{}/operstate", iface_name);
                if let Ok(state) = std::fs::read_to_string(&operstate_path) {
                    iface_status.state = state.trim().to_lowercase();
                }

                // Read MAC address
                let mac_path = format!("/sys/class/net/{}/address", iface_name);
                if let Ok(mac) = std::fs::read_to_string(&mac_path) {
                    iface_status.mac_address = mac.trim().to_string();
                }

                // Read MTU
                let mtu_path = format!("/sys/class/net/{}/mtu", iface_name);
                if let Ok(mtu_str) = std::fs::read_to_string(&mtu_path) {
                    if let Ok(mtu) = mtu_str.trim().parse::<u32>() {
                        iface_status.mtu = mtu;
                    }
                }

                // Get IP addresses using ip command
                if let Ok(output) = std::process::Command::new("/bin/ip")
                    .args(["-4", "addr", "show", iface_name])
                    .output()
                {
                    if let Ok(stdout) = String::from_utf8(output.stdout) {
                        for line in stdout.lines() {
                            if line.trim().starts_with("inet ") {
                                if let Some(addr) = line.split_whitespace().nth(1) {
                                    iface_status.ipv4_addresses.push(addr.to_string());
                                }
                            }
                        }
                    }
                }

                // Get IPv6 addresses using ip command
                if let Ok(output) = std::process::Command::new("/bin/ip")
                    .args(["-6", "addr", "show", iface_name])
                    .output()
                {
                    if let Ok(stdout) = String::from_utf8(output.stdout) {
                        for line in stdout.lines() {
                            if line.trim().starts_with("inet6 ") {
                                if let Some(addr) = line.split_whitespace().nth(1) {
                                    iface_status.ipv6_addresses.push(addr.to_string());
                                }
                            }
                        }
                    }
                }

                // Read statistics
                let rx_bytes_path = format!("/sys/class/net/{}/statistics/rx_bytes", iface_name);
                let tx_bytes_path = format!("/sys/class/net/{}/statistics/tx_bytes", iface_name);
                let rx_packets_path =
                    format!("/sys/class/net/{}/statistics/rx_packets", iface_name);
                let tx_packets_path =
                    format!("/sys/class/net/{}/statistics/tx_packets", iface_name);
                let rx_errors_path = format!("/sys/class/net/{}/statistics/rx_errors", iface_name);
                let tx_errors_path = format!("/sys/class/net/{}/statistics/tx_errors", iface_name);

                let rx_bytes = std::fs::read_to_string(&rx_bytes_path)
                    .ok()
                    .and_then(|s| s.trim().parse::<u64>().ok())
                    .unwrap_or(0);
                let tx_bytes = std::fs::read_to_string(&tx_bytes_path)
                    .ok()
                    .and_then(|s| s.trim().parse::<u64>().ok())
                    .unwrap_or(0);
                let rx_packets = std::fs::read_to_string(&rx_packets_path)
                    .ok()
                    .and_then(|s| s.trim().parse::<u64>().ok())
                    .unwrap_or(0);
                let tx_packets = std::fs::read_to_string(&tx_packets_path)
                    .ok()
                    .and_then(|s| s.trim().parse::<u64>().ok())
                    .unwrap_or(0);
                let rx_errors = std::fs::read_to_string(&rx_errors_path)
                    .ok()
                    .and_then(|s| s.trim().parse::<u64>().ok())
                    .unwrap_or(0);
                let tx_errors = std::fs::read_to_string(&tx_errors_path)
                    .ok()
                    .and_then(|s| s.trim().parse::<u64>().ok())
                    .unwrap_or(0);

                iface_status.statistics = Some(InterfaceStatistics {
                    rx_bytes,
                    tx_bytes,
                    rx_packets,
                    tx_packets,
                    rx_errors,
                    tx_errors,
                });

                interfaces.push(iface_status);
            }
        }
    }

    Ok(Response::new(GetNetworkStatusResponse { interfaces }))
}
