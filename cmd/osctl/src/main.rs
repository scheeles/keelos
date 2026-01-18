use clap::{Parser, Subcommand};
use matic_api::node::node_service_client::NodeServiceClient;
use matic_api::node::{GetStatusRequest, RebootRequest, InstallUpdateRequest};
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
    }

    Ok(())
}
