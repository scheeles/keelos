use clap::{Parser, Subcommand};
use keel_api::node::node_service_client::NodeServiceClient;
use keel_api::node::{
    AnalyzeCrashDumpRequest, BootstrapKubernetesRequest, CollectCrashDumpRequest,
    ConfigureNetworkRequest, CreateSystemSnapshotRequest, DhcpConfig, DnsConfig,
    EnableDebugModeRequest, EnableRecoveryModeRequest, GetBootstrapStatusRequest,
    GetDebugStatusRequest, GetHealthRequest, GetNetworkConfigRequest, GetNetworkStatusRequest,
    GetRollbackHistoryRequest, GetStatusRequest, InitBootstrapRequest, InstallUpdateRequest,
    NetworkInterface, RebootRequest, StaticConfig, StreamLogsRequest, TriggerRollbackRequest,
};
use std::path::PathBuf;
use tokio_stream::StreamExt;

mod cert_store;
use cert_store::{extract_node_from_endpoint, CertStore};

#[derive(Parser)]
#[command(name = "osctl")]
#[command(about = "KeelOS CLI Client", long_about = None)]
struct Cli {
    #[arg(long, default_value = "http://[::1]:50051")]
    endpoint: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Get node status
    Status,
    /// Reboot the node
    Reboot {
        #[arg(long, default_value = "Manual reboot via osctl")]
        reason: String,
    },
    /// Install an OS update
    Update {
        /// Source URL of the SquashFS image (or delta file if --delta is set)
        #[arg(long)]
        source: String,
        /// Expected SHA256 checksum
        #[arg(long)]
        sha256: Option<String>,
        /// Use delta update (source is a delta file)
        #[arg(long, default_value_t = false)]
        delta: bool,
        /// Enable fallback to full image if delta fails
        #[arg(long, default_value_t = false)]
        fallback: bool,
        /// URL for full image (used as fallback if delta fails)
        #[arg(long)]
        full_image_url: Option<String>,
    },
    /// Get system health status
    Health,
    /// Rollback operations
    Rollback {
        #[command(subcommand)]
        action: RollbackAction,
    },
    /// Initialize certificates
    Init {
        #[command(subcommand)]
        mode: InitMode,
    },
    /// Join a Kubernetes cluster
    Bootstrap {
        /// Kubernetes API server endpoint (e.g., "https://k8s.example.com:6443")
        #[arg(long)]
        api_server: String,
        /// Bootstrap token (format: <token-id>.<token-secret>)
        #[arg(long)]
        token: Option<String>,
        /// Path to cluster CA certificate
        #[arg(long)]
        ca_cert: Option<PathBuf>,
        /// Path to full kubeconfig file (alternative to token auth)
        #[arg(long)]
        kubeconfig: Option<PathBuf>,
        /// Override node name (default: hostname)
        #[arg(long)]
        node_name: Option<String>,
    },
    /// Get Kubernetes bootstrap status
    BootstrapStatus,
    /// Network management commands
    Network {
        #[command(subcommand)]
        action: NetworkAction,
    },
    /// Diagnostics and debugging commands
    Diag {
        #[command(subcommand)]
        action: DiagAction,
    },
}

#[derive(Subcommand)]
enum NetworkAction {
    /// Configure network settings
    Config {
        #[command(subcommand)]
        action: NetworkConfigAction,
    },
    /// Show network status
    Status,
    /// Configure DNS settings
    Dns {
        #[command(subcommand)]
        action: DnsAction,
    },
}

#[derive(Subcommand)]
enum NetworkConfigAction {
    /// Set network configuration
    Set {
        /// Interface name (e.g., eth0, ens3)
        #[arg(long)]
        interface: String,
        /// Use DHCP for this interface
        #[arg(long, conflicts_with_all = ["ip", "gateway", "ipv6", "ipv6_gateway"])]
        dhcp: bool,
        /// Static IPv4 address in CIDR notation (e.g., 192.168.1.10/24)
        #[arg(long)]
        ip: Option<String>,
        /// IPv4 gateway IP address
        #[arg(long)]
        gateway: Option<String>,
        /// IPv6 addresses in CIDR notation (can be specified multiple times)
        #[arg(long)]
        ipv6: Vec<String>,
        /// IPv6 gateway IP address
        #[arg(long)]
        ipv6_gateway: Option<String>,
        /// Enable IPv6 auto-configuration (SLAAC)
        #[arg(long)]
        ipv6_auto: bool,
        /// MTU (default: 1500)
        #[arg(long)]
        mtu: Option<u32>,
        /// Auto-reboot after configuration
        #[arg(long)]
        auto_reboot: bool,
    },
    /// Show current network configuration
    Show,
}

#[derive(Subcommand)]
enum DnsAction {
    /// Set DNS nameservers
    Set {
        /// DNS nameservers (can be specified multiple times)
        #[arg(long, required = true)]
        nameserver: Vec<String>,
        /// DNS search domains
        #[arg(long)]
        search: Vec<String>,
        /// Auto-reboot after configuration
        #[arg(long)]
        auto_reboot: bool,
    },
}

#[derive(Subcommand)]
enum RollbackAction {
    /// Manually trigger rollback to previous partition
    Trigger {
        /// Reason for rollback
        #[arg(long, default_value = "Manual rollback via osctl")]
        reason: String,
    },
    /// View rollback history
    History,
}

#[derive(Subcommand)]
enum InitMode {
    /// Initialize with self-signed bootstrap certificate (24h validity)
    Bootstrap {
        /// Node endpoint (e.g., "192.168.1.10", "localhost", or "localhost:50051")
        #[arg(long)]
        node: String,
    },
    /// Initialize with Kubernetes-signed operational certificate
    Kubeconfig,
}

#[derive(Subcommand)]
enum DiagAction {
    /// Enable time-limited debug mode
    Debug {
        /// Duration in seconds (max 3600, default 900)
        #[arg(long, default_value_t = 900)]
        duration: u32,
        /// Reason for enabling debug mode
        #[arg(long, default_value = "Manual debug via osctl")]
        reason: String,
    },
    /// Get current debug mode status
    DebugStatus,
    /// Collect crash dump (kernel + userspace info)
    CrashDump {
        /// Include kernel crash data (dmesg)
        #[arg(long, default_value_t = true)]
        kernel: bool,
        /// Include userspace process information
        #[arg(long, default_value_t = true)]
        userspace: bool,
    },
    /// Stream debug logs
    Logs {
        /// Filter by log level (debug, info, warn, error)
        #[arg(long, default_value = "")]
        level: String,
        /// Filter by component name
        #[arg(long, default_value = "")]
        component: String,
        /// Number of historical lines to include
        #[arg(long, default_value_t = 50)]
        tail: u32,
    },
    /// Create a system snapshot/backup
    Snapshot {
        /// Human-readable label for the snapshot
        #[arg(long, default_value = "manual snapshot")]
        label: String,
        /// Include system configuration files
        #[arg(long, default_value_t = true)]
        config: bool,
        /// Include logs
        #[arg(long, default_value_t = true)]
        logs: bool,
    },
    /// Enable emergency recovery mode
    Recovery {
        /// Duration in seconds (max 3600, default 900)
        #[arg(long, default_value_t = 900)]
        duration: u32,
        /// Reason for enabling recovery mode
        #[arg(long, default_value = "Manual recovery via osctl")]
        reason: String,
    },
    /// Analyze a previously collected crash dump
    AnalyzeDump {
        /// Path to the crash dump file to analyze
        #[arg(long)]
        path: String,
    },
}

/// Helper to create a TLS-enabled connection if certificates are available
/// Falls back to HTTP if no certs found
async fn connect_with_auto_tls(
    endpoint: &str,
) -> Result<NodeServiceClient<tonic::transport::Channel>, Box<dyn std::error::Error>> {
    let node_id = extract_node_from_endpoint(endpoint)?;
    let cert_store = CertStore::new()?;

    // Try to load saved certificates (prefer operational, fall back to bootstrap)
    if let Ok((tier, paths)) = cert_store.find_best_cert(&node_id) {
        eprintln!("🔐 Using {} certificates for mTLS", tier);

        // Load cert and key
        let cert_pem = std::fs::read_to_string(&paths.cert)?;
        let key_pem = std::fs::read_to_string(&paths.key)?;

        // Create TLS identity
        let identity = tonic::transport::Identity::from_pem(cert_pem, key_pem);

        // Configure TLS endpoint with timeout and keepalive
        let tls_endpoint = tonic::transport::Channel::from_shared(endpoint.to_string())?
            .tls_config(tonic::transport::ClientTlsConfig::new().identity(identity))?
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(30))
            .http2_keep_alive_interval(std::time::Duration::from_secs(10))
            .keep_alive_timeout(std::time::Duration::from_secs(20));

        Ok(NodeServiceClient::connect(tls_endpoint).await?)
    } else {
        // No certs found, use plain HTTP with timeout and keepalive
        eprintln!("ℹ️  No certificates found, using HTTP");
        eprintln!("💡 Run 'osctl init bootstrap --node <ip>' to enable mTLS");
        let channel = tonic::transport::Channel::from_shared(endpoint.to_string())?
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(30))
            .http2_keep_alive_interval(std::time::Duration::from_secs(10))
            .keep_alive_timeout(std::time::Duration::from_secs(20))
            .connect()
            .await?;
        Ok(NodeServiceClient::new(channel))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Install the rustls crypto provider required for TLS/mTLS connections.
    // Auto-detection from crate features can fail on musl static builds.
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    let cli = Cli::parse();

    // Auto-load certificates if available, fallback to HTTP
    let mut client = connect_with_auto_tls(&cli.endpoint).await?;

    match &cli.command {
        Commands::Status => {
            let request = tonic::Request::new(GetStatusRequest {});
            let response = client.get_status(request).await?;
            println!("RESPONSE={:?}", response.into_inner());
        }
        Commands::Reboot { reason } => {
            let request = tonic::Request::new(RebootRequest {
                reason: reason.clone(),
            });
            let response = client.reboot(request).await?;
            println!("Reboot Scheduled: {:?}", response.into_inner().scheduled);
        }
        Commands::Update {
            source,
            sha256,
            delta,
            fallback,
            full_image_url,
        } => {
            let request = tonic::Request::new(InstallUpdateRequest {
                source_url: source.clone(),
                expected_sha256: sha256.clone().unwrap_or_default(),
                is_delta: *delta,
                fallback_to_full: *fallback,
                full_image_url: full_image_url.clone().unwrap_or_default(),
            });
            let mut stream = client.install_update(request).await?.into_inner();
            while let Some(progress) = stream.next().await {
                let p = progress?;
                let phase_indicator = if !p.phase.is_empty() {
                    format!(" [{}]", p.phase)
                } else {
                    String::new()
                };
                println!("[{:>3}%]{} {}", p.percentage, phase_indicator, p.message);
                if p.success {
                    println!("Update complete!");
                    if p.bytes_saved > 0 {
                        let mb_saved = p.bytes_saved as f64 / (1024.0 * 1024.0);
                        println!("💾 Bandwidth saved: {:.2} MB", mb_saved);
                    }
                }
            }
        }
        Commands::Health => {
            let request = tonic::Request::new(GetHealthRequest {});
            let response = client.get_health(request).await?;
            let health = response.into_inner();

            println!("\n🏥 System Health: {}", health.status.to_uppercase());
            println!("Last Updated: {}\n", health.last_update_time);

            if !health.checks.is_empty() {
                println!("Health Checks:");
                for check in health.checks {
                    let icon = match check.status.as_str() {
                        "pass" => "✅",
                        "fail" => "❌",
                        _ => "⚠️",
                    };
                    println!(
                        "  {} {} - {} ({}ms)",
                        icon, check.name, check.message, check.duration_ms
                    );
                }
            }
        }
        Commands::Rollback { action } => match action {
            RollbackAction::Trigger { reason } => {
                let request = tonic::Request::new(TriggerRollbackRequest {
                    reason: reason.clone(),
                });
                let response = client.trigger_rollback(request).await?;
                let result = response.into_inner();

                if result.success {
                    println!("✅ {}", result.message);
                } else {
                    println!("❌ {}", result.message);
                }
            }
            RollbackAction::History => {
                let request = tonic::Request::new(GetRollbackHistoryRequest {});
                let response = client.get_rollback_history(request).await?;
                let history = response.into_inner();

                if history.events.is_empty() {
                    println!("No rollback events found.");
                } else {
                    println!("\n🔄 Rollback History ({} events):\n", history.events.len());
                    for event in history.events {
                        println!("  Timestamp: {}", event.timestamp);
                        println!("  Reason: {}", event.reason);
                        println!(
                            "  Type: {}",
                            if event.automatic {
                                "Automatic"
                            } else {
                                "Manual"
                            }
                        );
                        println!();
                    }
                }
            }
        },
        Commands::Bootstrap {
            api_server,
            token,
            ca_cert,
            kubeconfig,
            node_name,
        } => {
            // Validate inputs
            if token.is_none() && kubeconfig.is_none() {
                eprintln!("Error: Either --token or --kubeconfig must be provided");
                std::process::exit(1);
            }

            if token.is_some() && ca_cert.is_none() {
                eprintln!("Error: --ca-cert is required when using --token");
                std::process::exit(1);
            }

            // Read CA certificate if provided
            let ca_cert_pem = if let Some(path) = ca_cert {
                std::fs::read_to_string(path)?
            } else {
                String::new()
            };

            // Read kubeconfig if provided
            let kubeconfig_bytes = if let Some(path) = kubeconfig {
                std::fs::read(path)?
            } else {
                vec![]
            };

            let request = tonic::Request::new(BootstrapKubernetesRequest {
                api_server_endpoint: api_server.clone(),
                bootstrap_token: token.clone().unwrap_or_default(),
                ca_cert_pem,
                kubeconfig: kubeconfig_bytes,
                node_name: node_name.clone().unwrap_or_default(),
            });

            println!("🚀 Bootstrapping Kubernetes cluster connection...");
            let response = client.bootstrap_kubernetes(request).await?;
            let result = response.into_inner();

            if result.success {
                println!("\n✅ {}", result.message);
                println!("📄 Kubeconfig: {}", result.kubeconfig_path);
                println!(
                    "\n💡 Node will join the cluster shortly. Check with:\n   kubectl get nodes"
                );
            } else {
                eprintln!("\n❌ Bootstrap failed: {}", result.message);
                std::process::exit(1);
            }
        }
        Commands::Init { mode } => match mode {
            InitMode::Bootstrap { node } => {
                println!("Generating 24h bootstrap certificate...");

                let (cert_pem, key_pem) = keel_crypto::generate_bootstrap_certificate(24)?;
                println!("✓ Generated bootstrap certificate");

                let endpoint = if node.contains(':') {
                    format!("http://{node}")
                } else {
                    format!("http://{node}:50051")
                };
                let mut client = NodeServiceClient::connect(endpoint.clone()).await?;

                let request = tonic::Request::new(InitBootstrapRequest {
                    client_cert_pem: cert_pem.clone(),
                });

                let response = client.init_bootstrap(request).await?;
                let inner = response.into_inner();

                if !inner.success {
                    return Err(format!("Failed: {}", inner.message).into());
                }

                println!("✓ Server accepted bootstrap certificate");

                let node_id = extract_node_from_endpoint(&endpoint)?;
                let cert_store = CertStore::new()?;
                let paths =
                    cert_store.save_certs(&node_id, "bootstrap", &cert_pem, &key_pem, None)?;

                println!("✓ Saved certificates locally:");
                println!("  Cert: {}", paths.cert.display());
                println!(
                    "  Key:  {} (PRIVATE - never sent to server)",
                    paths.key.display()
                );
                println!("\n✅ Bootstrap initialization complete!");
            }
            InitMode::Kubeconfig => {
                println!("K8s operational cert initialization - TODO");
            }
        },
        Commands::BootstrapStatus => {
            let request = tonic::Request::new(GetBootstrapStatusRequest {});
            let response = client.get_bootstrap_status(request).await?;
            let status = response.into_inner();

            if status.is_bootstrapped {
                println!("\n✅ Node is bootstrapped to Kubernetes cluster\n");
                println!("API Server: {}", status.api_server_endpoint);
                println!("Node Name: {}", status.node_name);
                println!("Kubeconfig: {}", status.kubeconfig_path);
                println!("Bootstrapped At: {}", status.bootstrapped_at);
            } else {
                println!("\n⚠️  Node is not bootstrapped to any Kubernetes cluster");
                println!("\nTo join a cluster, run:\n   osctl bootstrap --api-server <url> --token <token> --ca-cert <path>");
            }
        }
        Commands::Network { action } => {
            match action {
                NetworkAction::Config { action } => match action {
                    NetworkConfigAction::Set {
                        interface,
                        dhcp,
                        ip,
                        gateway,
                        ipv6,
                        ipv6_gateway,
                        ipv6_auto,
                        mtu,
                        auto_reboot,
                    } => {
                        // Build network interface configuration
                        let iface_config = if *dhcp {
                            Some(keel_api::node::network_interface::Config::Dhcp(
                                DhcpConfig { enabled: true },
                            ))
                        } else if ip.is_some() || !ipv6.is_empty() || *ipv6_auto {
                            Some(keel_api::node::network_interface::Config::Static(
                                StaticConfig {
                                    ipv4_address: ip.clone().unwrap_or_default(),
                                    gateway: gateway.clone().unwrap_or_default(),
                                    mtu: mtu.unwrap_or(1500),
                                    ipv6_addresses: ipv6.clone(),
                                    ipv6_gateway: ipv6_gateway.clone().unwrap_or_default(),
                                    ipv6_auto: *ipv6_auto,
                                },
                            ))
                        } else {
                            eprintln!("Error: Either --dhcp, --ip, --ipv6, or --ipv6-auto must be specified");
                            std::process::exit(1);
                        };

                        let request = tonic::Request::new(ConfigureNetworkRequest {
                            interfaces: vec![NetworkInterface {
                                name: interface.clone(),
                                config: iface_config,
                            }],
                            dns: None,
                            routes: vec![],
                            auto_reboot: *auto_reboot,
                        });

                        println!("🌐 Configuring network interface '{}'...", interface);
                        let response = client.configure_network(request).await?;
                        let result = response.into_inner();

                        if result.success {
                            println!("✅ {}", result.message);
                            if result.reboot_required && !auto_reboot {
                                println!("\n⚠️  Reboot required for changes to take effect");
                                println!("   Run: osctl reboot");
                            }
                        } else {
                            eprintln!("❌ Configuration failed: {}", result.message);
                            std::process::exit(1);
                        }
                    }
                    NetworkConfigAction::Show => {
                        let request = tonic::Request::new(GetNetworkConfigRequest {});
                        let response = client.get_network_config(request).await?;
                        let config = response.into_inner();

                        if config.interfaces.is_empty()
                            && config.dns.is_none()
                            && config.routes.is_empty()
                        {
                            println!("No network configuration found (using DHCP fallback)");
                        } else {
                            println!("\n📡 Network Configuration:\n");
                            for iface in config.interfaces {
                                println!("Interface: {}", iface.name);
                                if let Some(cfg) = iface.config {
                                    match cfg {
                                        keel_api::node::network_interface::Config::Dhcp(_) => {
                                            println!("  Type: DHCP");
                                        }
                                        keel_api::node::network_interface::Config::Static(s) => {
                                            println!("  Type: Static");
                                            if !s.ipv4_address.is_empty() {
                                                println!("  IPv4: {}", s.ipv4_address);
                                                if !s.gateway.is_empty() {
                                                    println!("  IPv4 Gateway: {}", s.gateway);
                                                }
                                            }
                                            if !s.ipv6_addresses.is_empty() {
                                                println!("  IPv6: {}", s.ipv6_addresses.join(", "));
                                                if !s.ipv6_gateway.is_empty() {
                                                    println!("  IPv6 Gateway: {}", s.ipv6_gateway);
                                                }
                                            }
                                            println!("  MTU: {}", s.mtu);
                                        }
                                        keel_api::node::network_interface::Config::Vlan(v) => {
                                            println!("  Type: VLAN");
                                            println!("  Parent: {}", v.parent);
                                            println!("  VLAN ID: {}", v.vlan_id);
                                        }
                                        keel_api::node::network_interface::Config::Bond(b) => {
                                            println!("  Type: Bond");
                                            println!("  Mode: {}", b.mode);
                                            println!("  Slaves: {}", b.slaves.join(", "));
                                        }
                                    }
                                }
                                println!();
                            }

                            if let Some(dns) = config.dns {
                                println!("DNS Configuration:");
                                println!("  Nameservers: {}", dns.nameservers.join(", "));
                                if !dns.search_domains.is_empty() {
                                    println!("  Search domains: {}", dns.search_domains.join(", "));
                                }
                                println!();
                            }

                            if !config.routes.is_empty() {
                                println!("Custom Routes:");
                                for route in config.routes {
                                    print!("  {} via {}", route.destination, route.gateway);
                                    if route.metric > 0 {
                                        print!(" (metric: {})", route.metric);
                                    }
                                    println!();
                                }
                            }
                        }
                    }
                },
                NetworkAction::Status => {
                    let request = tonic::Request::new(GetNetworkStatusRequest {});
                    let response = client.get_network_status(request).await?;
                    let status = response.into_inner();

                    if status.interfaces.is_empty() {
                        println!("No network interfaces found");
                    } else {
                        println!("\n🌐 Network Status:\n");
                        for iface in status.interfaces {
                            let state_icon = match iface.state.as_str() {
                                "up" => "🟢",
                                "down" => "🔴",
                                _ => "⚪",
                            };
                            println!("{} {} ({})", state_icon, iface.name, iface.state);
                            println!("  MAC: {}", iface.mac_address);
                            println!("  MTU: {}", iface.mtu);

                            if !iface.ipv4_addresses.is_empty() {
                                println!("  IPv4: {}", iface.ipv4_addresses.join(", "));
                            }

                            if !iface.ipv6_addresses.is_empty() {
                                println!("  IPv6: {}", iface.ipv6_addresses.join(", "));
                            }

                            if let Some(stats) = iface.statistics {
                                let rx_mb = stats.rx_bytes as f64 / (1024.0 * 1024.0);
                                let tx_mb = stats.tx_bytes as f64 / (1024.0 * 1024.0);
                                println!(
                                    "  RX: {:.2} MB ({} packets, {} errors)",
                                    rx_mb, stats.rx_packets, stats.rx_errors
                                );
                                println!(
                                    "  TX: {:.2} MB ({} packets, {} errors)",
                                    tx_mb, stats.tx_packets, stats.tx_errors
                                );
                            }
                            println!();
                        }
                    }
                }
                NetworkAction::Dns { action } => match action {
                    DnsAction::Set {
                        nameserver,
                        search,
                        auto_reboot,
                    } => {
                        let request = tonic::Request::new(ConfigureNetworkRequest {
                            interfaces: vec![],
                            dns: Some(DnsConfig {
                                nameservers: nameserver.clone(),
                                search_domains: search.clone(),
                            }),
                            routes: vec![],
                            auto_reboot: *auto_reboot,
                        });

                        println!("🌐 Configuring DNS...");
                        let response = client.configure_network(request).await?;
                        let result = response.into_inner();

                        if result.success {
                            println!("✅ {}", result.message);
                            if result.reboot_required && !auto_reboot {
                                println!("\n⚠️  Reboot required for changes to take effect");
                                println!("   Run: osctl reboot");
                            }
                        } else {
                            eprintln!("❌ Configuration failed: {}", result.message);
                            std::process::exit(1);
                        }
                    }
                },
            }
        }
        Commands::Diag { action } => match action {
            DiagAction::Debug { duration, reason } => {
                let request = tonic::Request::new(EnableDebugModeRequest {
                    duration_secs: *duration,
                    reason: reason.clone(),
                });

                println!("🔧 Enabling debug mode...");
                let response = client.enable_debug_mode(request).await?;
                let result = response.into_inner();

                if result.success {
                    println!("✅ {}", result.message);
                    println!("  Session ID: {}", result.session_id);
                    println!("  Expires at: {}", result.expires_at);
                } else {
                    eprintln!("❌ {}", result.message);
                    std::process::exit(1);
                }
            }
            DiagAction::DebugStatus => {
                let request = tonic::Request::new(GetDebugStatusRequest {});
                let response = client.get_debug_status(request).await?;
                let status = response.into_inner();

                if status.enabled {
                    println!("\n🔧 Debug Mode: ACTIVE");
                    println!("  Session ID: {}", status.session_id);
                    println!("  Reason: {}", status.reason);
                    println!("  Expires at: {}", status.expires_at);
                    println!("  Remaining: {}s", status.remaining_secs);
                } else {
                    println!("\n🔧 Debug Mode: INACTIVE");
                }
            }
            DiagAction::CrashDump { kernel, userspace } => {
                let request = tonic::Request::new(CollectCrashDumpRequest {
                    include_kernel: *kernel,
                    include_userspace: *userspace,
                });

                println!("📦 Collecting crash dump...");
                let response = client.collect_crash_dump(request).await?;
                let result = response.into_inner();

                if result.success {
                    println!("✅ {}", result.message);
                    println!("  Path: {}", result.dump_path);
                    let kb = result.dump_size_bytes as f64 / 1024.0;
                    println!("  Size: {kb:.2} KB");
                    println!("  Created: {}", result.created_at);
                } else {
                    eprintln!("❌ {}", result.message);
                    std::process::exit(1);
                }
            }
            DiagAction::Logs {
                level,
                component,
                tail,
            } => {
                let request = tonic::Request::new(StreamLogsRequest {
                    level: level.clone(),
                    component: component.clone(),
                    tail_lines: *tail,
                });

                println!("📜 Streaming logs...\n");
                let mut stream = client.stream_logs(request).await?.into_inner();
                while let Some(entry) = stream.next().await {
                    let entry = entry?;
                    println!(
                        "[{}] {} [{}] {}",
                        entry.timestamp, entry.level, entry.component, entry.message
                    );
                }
            }
            DiagAction::Snapshot {
                label,
                config,
                logs,
            } => {
                let request = tonic::Request::new(CreateSystemSnapshotRequest {
                    label: label.clone(),
                    include_config: *config,
                    include_logs: *logs,
                });

                println!("📸 Creating system snapshot...");
                let response = client.create_system_snapshot(request).await?;
                let result = response.into_inner();

                if result.success {
                    println!("✅ {}", result.message);
                    println!("  Snapshot ID: {}", result.snapshot_id);
                    println!("  Path: {}", result.snapshot_path);
                    let kb = result.size_bytes as f64 / 1024.0;
                    println!("  Size: {kb:.2} KB");
                    println!("  Created: {}", result.created_at);
                } else {
                    eprintln!("❌ {}", result.message);
                    std::process::exit(1);
                }
            }
            DiagAction::Recovery { duration, reason } => {
                let request = tonic::Request::new(EnableRecoveryModeRequest {
                    duration_secs: *duration,
                    reason: reason.clone(),
                });

                println!("🚨 Enabling recovery mode...");
                let response = client.enable_recovery_mode(request).await?;
                let result = response.into_inner();

                if result.success {
                    println!("✅ {}", result.message);
                    println!("  Expires at: {}", result.expires_at);
                } else {
                    eprintln!("❌ {}", result.message);
                    std::process::exit(1);
                }
            }
            DiagAction::AnalyzeDump { path } => {
                let request = tonic::Request::new(AnalyzeCrashDumpRequest {
                    dump_path: path.clone(),
                });

                println!("🔍 Analyzing crash dump...");
                let response = client.analyze_crash_dump(request).await?;
                let result = response.into_inner();

                if result.success {
                    println!("✅ {}", result.message);
                    println!("  Severity: {}", result.severity);
                    println!("  Summary: {}", result.summary);
                    if !result.findings.is_empty() {
                        println!("\n  Findings:");
                        for finding in &result.findings {
                            println!(
                                "    [{}/{}] {}",
                                finding.severity, finding.finding_type, finding.message
                            );
                        }
                    }
                } else {
                    eprintln!("❌ {}", result.message);
                    std::process::exit(1);
                }
            }
        },
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn test_cli_parsing_status() {
        let cli = Cli::try_parse_from(["osctl", "status"]).unwrap();
        assert!(matches!(cli.command, Commands::Status));
        assert_eq!(cli.endpoint, "http://[::1]:50051");
    }

    #[test]
    fn test_cli_parsing_status_custom_endpoint() {
        let cli = Cli::try_parse_from(["osctl", "--endpoint", "http://localhost:9000", "status"])
            .unwrap();
        assert!(matches!(cli.command, Commands::Status));
        assert_eq!(cli.endpoint, "http://localhost:9000");
    }

    #[test]
    fn test_cli_parsing_reboot() {
        let cli = Cli::try_parse_from(["osctl", "reboot"]).unwrap();
        if let Commands::Reboot { reason } = cli.command {
            assert_eq!(reason, "Manual reboot via osctl");
        } else {
            panic!("Expected Reboot command");
        }
    }

    #[test]
    fn test_cli_parsing_reboot_custom_reason() {
        let cli = Cli::try_parse_from(["osctl", "reboot", "--reason", "Maintenance"]).unwrap();
        if let Commands::Reboot { reason } = cli.command {
            assert_eq!(reason, "Maintenance");
        } else {
            panic!("Expected Reboot command");
        }
    }

    #[test]
    fn test_cli_parsing_update() {
        let cli = Cli::try_parse_from([
            "osctl",
            "update",
            "--source",
            "http://example.com/image.squashfs",
        ])
        .unwrap();
        if let Commands::Update { source, sha256, .. } = cli.command {
            assert_eq!(source, "http://example.com/image.squashfs");
            assert!(sha256.is_none());
        } else {
            panic!("Expected Update command");
        }
    }

    #[test]
    fn test_cli_parsing_update_with_sha256() {
        let cli = Cli::try_parse_from([
            "osctl",
            "update",
            "--source",
            "http://example.com/image.squashfs",
            "--sha256",
            "abc123def456",
        ])
        .unwrap();
        if let Commands::Update { source, sha256, .. } = cli.command {
            assert_eq!(source, "http://example.com/image.squashfs");
            assert_eq!(sha256, Some("abc123def456".to_string()));
        } else {
            panic!("Expected Update command");
        }
    }

    #[test]
    fn test_cli_parsing_update_delta() {
        let cli = Cli::try_parse_from([
            "osctl",
            "update",
            "--source",
            "http://example.com/update.delta",
            "--delta",
            "--fallback",
            "--full-image-url",
            "http://example.com/full.img",
        ])
        .unwrap();

        if let Commands::Update {
            source,
            delta,
            fallback,
            full_image_url,
            ..
        } = cli.command
        {
            assert_eq!(source, "http://example.com/update.delta");
            assert!(delta);
            assert!(fallback);
            assert_eq!(
                full_image_url,
                Some("http://example.com/full.img".to_string())
            );
        } else {
            panic!("Expected Update command");
        }
    }

    #[test]
    fn test_cli_missing_update_source() {
        let result = Cli::try_parse_from(["osctl", "update"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_cli_help_available() {
        // Verify help is available without panicking
        let _cmd = Cli::command();
    }

    #[test]
    fn test_cli_parsing_health() {
        let cli = Cli::try_parse_from(["osctl", "health"]).unwrap();
        assert!(matches!(cli.command, Commands::Health));
    }

    #[test]
    fn test_cli_parsing_rollback_trigger() {
        let cli = Cli::try_parse_from(["osctl", "rollback", "trigger"]).unwrap();
        if let Commands::Rollback { action } = cli.command {
            assert!(matches!(action, RollbackAction::Trigger { .. }));
        } else {
            panic!("Expected Rollback command");
        }
    }

    #[test]
    fn test_cli_parsing_rollback_history() {
        let cli = Cli::try_parse_from(["osctl", "rollback", "history"]).unwrap();
        if let Commands::Rollback { action } = cli.command {
            assert!(matches!(action, RollbackAction::History));
        } else {
            panic!("Expected Rollback command");
        }
    }

    #[test]
    fn test_cli_parsing_diag_debug() {
        let cli = Cli::try_parse_from(["osctl", "diag", "debug"]).unwrap();
        if let Commands::Diag {
            action: DiagAction::Debug { duration, reason },
        } = cli.command
        {
            assert_eq!(duration, 900);
            assert_eq!(reason, "Manual debug via osctl");
        } else {
            panic!("Expected Diag Debug command");
        }
    }

    #[test]
    fn test_cli_parsing_diag_debug_custom() {
        let cli = Cli::try_parse_from([
            "osctl",
            "diag",
            "debug",
            "--duration",
            "600",
            "--reason",
            "investigating issue",
        ])
        .unwrap();
        if let Commands::Diag {
            action: DiagAction::Debug { duration, reason },
        } = cli.command
        {
            assert_eq!(duration, 600);
            assert_eq!(reason, "investigating issue");
        } else {
            panic!("Expected Diag Debug command");
        }
    }

    #[test]
    fn test_cli_parsing_diag_debug_status() {
        let cli = Cli::try_parse_from(["osctl", "diag", "debug-status"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Diag {
                action: DiagAction::DebugStatus
            }
        ));
    }

    #[test]
    fn test_cli_parsing_diag_crash_dump() {
        let cli = Cli::try_parse_from(["osctl", "diag", "crash-dump"]).unwrap();
        if let Commands::Diag {
            action: DiagAction::CrashDump { kernel, userspace },
        } = cli.command
        {
            assert!(kernel);
            assert!(userspace);
        } else {
            panic!("Expected Diag CrashDump command");
        }
    }

    #[test]
    fn test_cli_parsing_diag_logs() {
        let cli =
            Cli::try_parse_from(["osctl", "diag", "logs", "--level", "error", "--tail", "100"])
                .unwrap();
        if let Commands::Diag {
            action:
                DiagAction::Logs {
                    level,
                    component,
                    tail,
                },
        } = cli.command
        {
            assert_eq!(level, "error");
            assert!(component.is_empty());
            assert_eq!(tail, 100);
        } else {
            panic!("Expected Diag Logs command");
        }
    }

    #[test]
    fn test_cli_parsing_diag_snapshot() {
        let cli =
            Cli::try_parse_from(["osctl", "diag", "snapshot", "--label", "pre-upgrade"]).unwrap();
        if let Commands::Diag {
            action:
                DiagAction::Snapshot {
                    label,
                    config,
                    logs,
                },
        } = cli.command
        {
            assert_eq!(label, "pre-upgrade");
            assert!(config);
            assert!(logs);
        } else {
            panic!("Expected Diag Snapshot command");
        }
    }

    #[test]
    fn test_cli_parsing_diag_recovery() {
        let cli = Cli::try_parse_from([
            "osctl",
            "diag",
            "recovery",
            "--duration",
            "1800",
            "--reason",
            "emergency repair",
        ])
        .unwrap();
        if let Commands::Diag {
            action: DiagAction::Recovery { duration, reason },
        } = cli.command
        {
            assert_eq!(duration, 1800);
            assert_eq!(reason, "emergency repair");
        } else {
            panic!("Expected Diag Recovery command");
        }
    }

    #[test]
    fn test_cli_parsing_diag_analyze_dump() {
        let cli = Cli::try_parse_from([
            "osctl",
            "diag",
            "analyze-dump",
            "--path",
            "/var/lib/keel/crash-dumps/crash-20240101-120000.txt",
        ])
        .unwrap();
        if let Commands::Diag {
            action: DiagAction::AnalyzeDump { path },
        } = cli.command
        {
            assert_eq!(path, "/var/lib/keel/crash-dumps/crash-20240101-120000.txt");
        } else {
            panic!("Expected Diag AnalyzeDump command");
        }
    }
}
