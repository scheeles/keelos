use tonic::{transport::Server, Request, Response, Status};
use matic_api::node::node_service_server::{NodeService, NodeServiceServer};
use matic_api::node::{GetStatusRequest, GetStatusResponse, RebootRequest, RebootResponse};

#[derive(Debug, Default)]
pub struct HelperNodeService {}

#[tonic::async_trait]
impl NodeService for HelperNodeService {
    async fn get_status(
        &self,
        _request: Request<GetStatusRequest>,
    ) -> Result<Response<GetStatusResponse>, Status> {
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
        println!("Reboot requested: {}", request.into_inner().reason);
        // In real impl, checking authZ then shelling out to reboot or trigger syscall
        Ok(Response::new(RebootResponse { scheduled: true }))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = "[::0]:50051".parse()?;
    let node_service = HelperNodeService::default();

    println!("Matic Agent starting on {}", addr);

    // Load declarative configuration
    let config_path = "/etc/matic/node.yaml";
    let config = if std::path::Path::new(config_path).exists() {
        println!("Loading configuration from {}...", config_path);
        matic_config::NodeConfig::load(config_path)?
    } else {
        println!("WARNING: Configuration not found at {}. Using defaults.", config_path);
        matic_config::NodeConfig::default_config()
    };
    println!("Hostname: {}", config.hostname);

    // mTLS setup
    // For now, we assume certs are provided via /etc/matic/crypto or similar.
    // In a real bootstrap, the Agent would wait for these or generate them if requested.
    let cert_path = "/etc/matic/crypto/server.pem";
    let key_path = "/etc/matic/crypto/server.key";
    let ca_path = "/etc/matic/crypto/ca.pem";

    let mut builder = Server::builder();

    if std::path::Path::new(cert_path).exists() {
        println!("Enabling mTLS...");
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
        println!("WARNING: Running without TLS (Certs missing at {})", cert_path);
    }

    builder
        .add_service(NodeServiceServer::new(node_service))
        .serve(addr)
        .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use matic_api::node_service_server::NodeService;
    use matic_api::GetStatusRequest;

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

