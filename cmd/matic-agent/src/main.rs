//! MaticOS Agent - gRPC management server
//!
//! The Matic Agent provides a gRPC API for managing the node, including:
//! - Node status queries
//! - Reboot scheduling
//! - A/B partition updates

use tonic::{transport::Server, Request, Response, Status};
use matic_api::node::node_service_server::{NodeService, NodeServiceServer};
use matic_api::node::{GetStatusRequest, GetStatusResponse, RebootRequest, RebootResponse, InstallUpdateRequest, UpdateProgress};
use std::pin::Pin;
use tokio_stream::Stream;
use tracing::{info, warn, debug, Level};
use tracing_subscriber::FmtSubscriber;

mod disk;

#[derive(Debug, Default)]
pub struct HelperNodeService {}

#[tonic::async_trait]
impl NodeService for HelperNodeService {
    async fn get_status(
        &self,
        _request: Request<GetStatusRequest>,
    ) -> Result<Response<GetStatusResponse>, Status> {
        debug!("Received get_status request");
        let reply = GetStatusResponse {
            hostname: "matic-node".to_string(), // TODO: Get from hostname
            kernel_version: "6.6.14".to_string(), // TODO: Get from uname
            os_version: "0.1.0".to_string(),
            uptime_seconds: 0.0, // TODO: Get from /proc/uptime
        };
        Ok(Response::new(reply))
    }

    async fn reboot(
        &self,
        request: Request<RebootRequest>,
    ) -> Result<Response<RebootResponse>, Status> {
        let reason = request.into_inner().reason;
        info!(reason = %reason, "Reboot requested");
        // In real impl, checking authZ then shelling out to reboot or trigger syscall
        Ok(Response::new(RebootResponse { scheduled: true }))
    }

    type InstallUpdateStream = Pin<Box<dyn Stream<Item = Result<UpdateProgress, Status>> + Send>>;

    async fn install_update(
        &self,
        request: Request<InstallUpdateRequest>,
    ) -> Result<Response<Self::InstallUpdateStream>, Status> {
        let req = request.into_inner();
        let source_url = req.source_url.clone();
        let expected_sha256 = if req.expected_sha256.is_empty() {
            None
        } else {
            Some(req.expected_sha256.clone())
        };
        info!(source = %source_url, has_sha256 = expected_sha256.is_some(), "Install update requested");

        let output = async_stream::try_stream! {
            yield UpdateProgress {
                percentage: 0,
                message: "Identifying target partition...".to_string(),
                success: false,
            };

            let inactive = disk::get_inactive_partition()
                .map_err(|e| Status::internal(format!("Failed to get inactive partition: {}", e)))?;
            
            debug!(device = %inactive.device, index = inactive.index, "Identified inactive partition");
            
            yield UpdateProgress {
                percentage: 10,
                message: format!("Target partition identified: {}", inactive.device),
                success: false,
            };

            yield UpdateProgress {
                percentage: 20,
                message: format!("Downloading and flashing to {}...", inactive.device),
                success: false,
            };

            // Disk flashing with optional SHA256 verification
            disk::flash_image(&source_url, &inactive.device, expected_sha256.as_deref()).await
                .map_err(|e| Status::internal(format!("Flash error: {}", e)))?;

            yield UpdateProgress {
                percentage: 80,
                message: "Image flashed. Toggling boot flags...".to_string(),
                success: false,
            };

            disk::switch_boot_partition(inactive.index)
                .map_err(|e| Status::internal(format!("Failed to switch boot partition: {}", e)))?;

            info!(target_partition = inactive.index, "Update installed successfully");
            
            yield UpdateProgress {
                percentage: 100,
                message: "Update installed successfully. Reboot to apply.".to_string(),
                success: true,
            };
        };

        Ok(Response::new(Box::pin(output) as Self::InstallUpdateStream))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing subscriber
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_target(true)
        .compact()
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    let addr = "0.0.0.0:50051".parse()?;
    let node_service = HelperNodeService::default();

    info!(addr = %addr, "Matic Agent starting");

    // Load declarative configuration
    let config_path = "/etc/matic/node.yaml";
    let config = if std::path::Path::new(config_path).exists() {
        info!(path = config_path, "Loading configuration");
        matic_config::NodeConfig::load(config_path)?
    } else {
        warn!(path = config_path, "Configuration not found, using defaults");
        matic_config::NodeConfig::default_config()
    };
    info!(hostname = %config.hostname, "Configuration loaded");

    // mTLS setup
    let cert_path = "/etc/matic/crypto/server.pem";
    let key_path = "/etc/matic/crypto/server.key";
    let ca_path = "/etc/matic/crypto/ca.pem";

    let mut builder = Server::builder();

    if std::path::Path::new(cert_path).exists() {
        info!("Enabling mTLS");
        let cert = std::fs::read_to_string(cert_path)?;
        let key = std::fs::read_to_string(key_path)?;
        let client_ca = std::fs::read_to_string(ca_path)?;

        let identity = tonic::transport::Identity::from_pem(cert, key);
        let client_ca_cert = tonic::transport::Certificate::from_pem(client_ca);

        let tls_config = tonic::transport::ServerTlsConfig::new()
            .identity(identity)
            .client_ca_root(client_ca_cert);

        builder = builder.tls_config(tls_config)?;
    } else {
        warn!(cert_path = cert_path, "Running without TLS - certificates not found");
    }

    info!("gRPC server ready");
    builder
        .add_service(NodeServiceServer::new(node_service))
        .serve(addr)
        .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use matic_api::node::node_service_server::NodeService;
    use matic_api::node::GetStatusRequest;

    #[tokio::test]
    async fn test_get_status() {
        let service = HelperNodeService::default();
        let request = tonic::Request::new(GetStatusRequest {});
        let response = service.get_status(request).await.unwrap();
        let inner = response.into_inner();
        
        assert_eq!(inner.hostname, "matic-node");
        assert_eq!(inner.os_version, "0.1.0");
    }
}
