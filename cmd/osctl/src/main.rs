use clap::{Parser, Subcommand};
use matic_api::node::node_service_client::NodeServiceClient;
use matic_api::node::{
    GetHealthRequest, GetRollbackHistoryRequest, GetStatusRequest, InstallUpdateRequest,
    RebootRequest, TriggerRollbackRequest,
};
use tokio_stream::StreamExt;

#[derive(Parser)]
#[command(name = "osctl")]
#[command(about = "MaticOS CLI Client", long_about = None)]
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
        /// Source URL of the SquashFS image
        #[arg(long)]
        source: String,
        /// Expected SHA256 checksum
        #[arg(long)]
        sha256: Option<String>,
    },
    /// Get system health status
    Health,
    /// Rollback operations
    Rollback {
        #[command(subcommand)]
        action: RollbackAction,
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
        Commands::Update { source, sha256 } => {
            let request = tonic::Request::new(InstallUpdateRequest {
                source_url: source.clone(),
                expected_sha256: sha256.clone().unwrap_or_default(),
            });
            let mut stream = client.install_update(request).await?.into_inner();
            while let Some(progress) = stream.next().await {
                let p = progress?;
                println!("[{:>3}%] {}", p.percentage, p.message);
                if p.success {
                    println!("Update complete!");
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
        if let Commands::Update { source, sha256 } = cli.command {
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
        if let Commands::Update { source, sha256 } = cli.command {
            assert_eq!(source, "http://example.com/image.squashfs");
            assert_eq!(sha256, Some("abc123def456".to_string()));
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
