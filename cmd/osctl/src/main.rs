use clap::{Parser, Subcommand};
use matic_api::node::node_service_client::NodeServiceClient;
use matic_api::node::{GetStatusRequest, RebootRequest};

#[derive(Parser)]
#[command(name = "osctl")]
#[command(about = "MaticOS CLI Client", long_about = None)]
struct Cli {
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
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    let cert_path = "client.pem";
    let key_path = "client.key";
    let ca_path = "ca.pem";

    let mut endpoint = tonic::transport::Endpoint::from_static("http://[::1]:50051");

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
    }

    Ok(())
}
