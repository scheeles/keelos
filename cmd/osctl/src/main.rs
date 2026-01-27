use clap::{Parser, Subcommand};
use keel_api::node::node_service_client::NodeServiceClient;
use keel_api::node::{
    BootstrapKubernetesRequest, GetBootstrapStatusRequest, GetHealthRequest,
    GetRollbackHistoryRequest, GetStatusRequest, InstallUpdateRequest, RebootRequest,
    TriggerRollbackRequest,
};
use std::path::PathBuf;
use tokio_stream::StreamExt;
use std::path::PathBuf;

mod init;

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
    /// Initialize osctl certificates
    Init {
        /// Bootstrap mode: retrieve initial certificates from node
        #[arg(long)]
        bootstrap: bool,
        
        /// Node IP address (required for bootstrap mode)
        #[arg(long)]
        node: Option<String>,
        
        /// Path to kubeconfig (for K8s PKI mode)
        #[arg(long)]
        kubeconfig: Option<String>,
        
        /// Certificate directory (default: ~/.keel)
        #[arg(long)]
        cert_dir: Option<String>,
        
        /// Certificate name/CN (default: osctl-user)
        #[arg(long, default_value = "osctl-user")]
        cert_name: String,
        
        /// Auto-approve CSR (K8s mode, requires admin permissions)
        #[arg(long)]
        auto_approve: bool,
    },
    /// Rollback operations
    Rollback {
        #[command(subcommand)]
        action: RollbackAction,
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    let cert_path = "client.pem";
    let key_path = "client.key";
    let ca_path = "ca.pem";

    let mut endpoint = tonic::transport::Endpoint::from_shared(cli.endpoint.clone())?;

    if std::path::Path::new(cert_path).exists() {
        let cert = std::fs::read_to_string(cert_path)?;
        let key = std::fs::read_to_string(key_path)?;
        let ca = std::fs::read_to_string(ca_path)?;

        let identity = tonic::transport::Identity::from_pem(cert, key);
        let ca_cert = tonic::transport::Certificate::from_pem(ca);

        let tls_config = tonic::transport::ClientTlsConfig::new()
            .identity(identity)
            .ca_certificate(ca_cert)
            .domain_name("localhost");

        endpoint = endpoint.tls_config(tls_config)?;
    }

    let mut client = NodeServiceClient::connect(endpoint).await?;

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
        Commands::Init {
            bootstrap,
            node,
            kubeconfig,
            cert_dir,
            cert_name,
            auto_approve,
        } => {
            // Determine certificate directory
            let cert_path = if let Some(dir) = cert_dir {
                PathBuf::from(dir)
            } else {
                dirs::home_dir()
                    .ok_or_else(|| anyhow::anyhow!("Could not determine home directory"))?
                    .join(".keel")
            };

            if *bootstrap {
                // Bootstrap mode: get initial certificates from node
                let node_addr = node
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("--node required for bootstrap mode"))?;
                init::init_bootstrap(node_addr, cert_path).await?;
            } else {
                // K8s PKI mode: get operational certificates
                let kubeconfig_path = kubeconfig
                    .as_ref()
                    .map(|s| s.as_str())
                    .unwrap_or("~/.kube/config");
                init::init_kubernetes(kubeconfig_path, cert_path, cert_name, *auto_approve).await?;
            }
            return Ok(());
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
}
