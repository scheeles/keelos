//! MaticOS Agent - gRPC management server
//!
//! The Matic Agent provides a gRPC API for managing the node, including:
//! - Node status queries
//! - Reboot scheduling
//! - A/B partition updates
//! - Scheduled updates with maintenance windows
//!
//! Additionally provides HTTP endpoints for:
//! - /healthz - Liveness checks
//! - /readyz - Readiness checks
//! - /metrics - Prometheus metrics

use matic_api::node::node_service_server::{NodeService, NodeServiceServer};
use matic_api::node::{
    CancelScheduledUpdateRequest, CancelScheduledUpdateResponse, GetStatusRequest,
    GetStatusResponse, GetUpdateScheduleRequest, GetUpdateScheduleResponse, InstallUpdateRequest,
    RebootRequest, RebootResponse, ScheduleUpdateRequest, ScheduleUpdateResponse, UpdateProgress,
    UpdateSchedule as ProtoUpdateSchedule,
};
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio_stream::Stream;
use tonic::{transport::Server, Request, Response, Status};
use tracing::{debug, info, warn};

mod disk;
mod health;
mod telemetry;
mod update_scheduler;
mod health_check;


use update_scheduler::{UpdateScheduler, ScheduleStatus};

#[derive(Debug, Clone)]
pub struct HelperNodeService {
    scheduler: Arc<UpdateScheduler>,
}

#[tonic::async_trait]
impl NodeService for HelperNodeService {
    async fn get_status(
        &self,
        _request: Request<GetStatusRequest>,
    ) -> Result<Response<GetStatusResponse>, Status> {
        debug!("Received get_status request");
        let reply = GetStatusResponse {
            hostname: "matic-node".to_string(),   // TODO: Get from hostname
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

    async fn schedule_update(
        &self,
        request: Request<ScheduleUpdateRequest>,
    ) -> Result<Response<ScheduleUpdateResponse>, Status> {
        let req = request.into_inner();
        
        info!(
            source = %req.source_url,
            scheduled_at = %req.scheduled_at,
            "Schedule update requested"
        );

        // Parse scheduled_at if provided
        let scheduled_at = if !req.scheduled_at.is_empty() {
            Some(
                chrono::DateTime::parse_from_rfc3339(&req.scheduled_at)
                    .map_err(|e| Status::invalid_argument(format!("Invalid scheduled_at: {}", e)))?
                    .with_timezone(&chrono::Utc),
            )
        } else {
            None
        };

        let maintenance_window = if req.maintenance_window_secs > 0 {
            Some(req.maintenance_window_secs)
        } else {
            None
        };

        let expected_sha256 = if !req.expected_sha256.is_empty() {
            Some(req.expected_sha256)
        } else {
            None
        };

        let pre_hook = if !req.pre_update_hook.is_empty() {
            Some(req.pre_update_hook)
        } else {
            None
        };

        let post_hook = if !req.post_update_hook.is_empty() {
            Some(req.post_update_hook)
        } else {
            None
        };

        let schedule = self
            .scheduler
            .schedule_update(
                req.source_url,
                expected_sha256,
                scheduled_at,
                maintenance_window,
                req.enable_auto_rollback,
                pre_hook,
                post_hook,
            )
            .await
            .map_err(|e| Status::internal(format!("Failed to schedule update: {}", e)))?;

        Ok(Response::new(ScheduleUpdateResponse {
            schedule_id: schedule.id.clone(),
            status: schedule.status.to_string(),
            scheduled_at: schedule
                .scheduled_at
                .map(|dt| dt.to_rfc3339())
                .unwrap_or_default(),
        }))
    }

    async fn get_update_schedule(
        &self,
        _request: Request<GetUpdateScheduleRequest>,
    ) -> Result<Response<GetUpdateScheduleResponse>, Status> {
        debug!("Get update schedule requested");

        let schedules = self.scheduler.get_schedules().await;

        let proto_schedules: Vec<ProtoUpdateSchedule> = schedules
            .into_iter()
            .map(|s| ProtoUpdateSchedule {
                id: s.id,
                source_url: s.source_url,
                expected_sha256: s.expected_sha256.unwrap_or_default(),
                scheduled_at: s.scheduled_at.map(|dt| dt.to_rfc3339()).unwrap_or_default(),
                status: s.status.to_string(),
                enable_auto_rollback: s.enable_auto_rollback,
                created_at: s.created_at.to_rfc3339(),
            })
            .collect();

        Ok(Response::new(GetUpdateScheduleResponse {
            schedules: proto_schedules,
        }))
    }

    async fn cancel_scheduled_update(
        &self,
        request: Request<CancelScheduledUpdateRequest>,
    ) -> Result<Response<CancelScheduledUpdateResponse>, Status> {
        let req = request.into_inner();

        info!(schedule_id = %req.schedule_id, "Cancel scheduled update requested");

        match self.scheduler.cancel_schedule(&req.schedule_id).await {
            Ok(_) => Ok(Response::new(CancelScheduledUpdateResponse {
                success: true,
                message: "Update cancelled successfully".to_string(),
            })),
            Err(e) => Ok(Response::new(CancelScheduledUpdateResponse {
                success: false,
                message: e,
            })),
        }
    }
}


#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize OpenTelemetry telemetry
    let otlp_endpoint = std::env::var("OTLP_ENDPOINT").ok();
    telemetry::init_telemetry("matic-agent", otlp_endpoint)?;

    let grpc_addr = "0.0.0.0:50051".parse()?;
    let health_addr = "0.0.0.0:9090".parse()?;
    
    // Initialize update scheduler
    let scheduler = Arc::new(UpdateScheduler::new("/var/lib/matic/update-schedule.json"));
    
    // Start background executor for scheduled updates
    let executor_scheduler = scheduler.clone();
    tokio::spawn(async move {
        schedule_executor(executor_scheduler).await;
    });

    let node_service = HelperNodeService {
        scheduler: scheduler.clone(),
    };

    info!(grpc_addr = %grpc_addr, "Matic Agent starting");

    // Load declarative configuration
    let config_path = "/etc/matic/node.yaml";
    let config = if std::path::Path::new(config_path).exists() {
        info!(path = config_path, "Loading configuration");
        matic_config::NodeConfig::load(config_path)?
    } else {
        warn!(
            path = config_path,
            "Configuration not found, using defaults"
        );
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
        warn!(
            cert_path = cert_path,
            "Running without TLS - certificates not found"
        );
    }

    // Start health/metrics HTTP server
    let metrics = Arc::new(RwLock::new(telemetry::SystemMetrics::default()));
    let health_state = Arc::new(health::HealthState {
        metrics: metrics.clone(),
    });
    let health_router = health::create_health_router(health_state);

    let health_server = tokio::spawn(async move {
        info!("Starting health/metrics HTTP server");
        let listener = tokio::net::TcpListener::bind(health_addr)
            .await
            .expect("Failed to bind health server");
        axum::serve(listener, health_router)
            .await
            .expect("Health server failed");
    });

    // Start gRPC server
    info!(addr = %grpc_addr, "Starting gRPC server");
    let grpc_server = builder
        .add_service(NodeServiceServer::new(node_service))
        .serve(grpc_addr);

    // Run both servers concurrently
    tokio::select! {
        result = grpc_server => {
            if let Err(e) = result {
                warn!(error = %e, "gRPC server error");
            }
        }
        result = health_server => {
            if let Err(e) = result {
                warn!(error = %e, "Health server error");
            }
        }
    }

    // Shutdown telemetry
    telemetry::shutdown_telemetry();

    Ok(())
}

/// Background task executor for scheduled updates
async fn schedule_executor(scheduler: Arc<UpdateScheduler>) {
    use tokio::time::{sleep, Duration};

    info!("Background schedule executor started");

    loop {
        // Check for due schedules every 30 seconds
        sleep(Duration::from_secs(30)).await;

        let due_schedules = scheduler.get_due_schedules().await;

        for schedule in due_schedules {
            info!(
                schedule_id = %schedule.id,
                source = %schedule.source_url,
                "Executing scheduled update"
            );

            // Mark as running
            let _ = scheduler
                .update_status(&schedule.id, ScheduleStatus::Running, None)
                .await;

            // Execute the update (simplified - in real implementation would use install_update logic)
            match execute_scheduled_update(&schedule).await {
                Ok(_) => {
                    info!(schedule_id = %schedule.id, "Scheduled update completed successfully");
                    let _ = scheduler
                        .update_status(&schedule.id, ScheduleStatus::Completed, None)
                        .await;
                }
                Err(e) => {
                    error!(schedule_id = %schedule.id, error = %e, "Scheduled update failed");
                    let _ = scheduler
                        .update_status(&schedule.id, ScheduleStatus::Failed, Some(e.to_string()))
                        .await;
                }
            }
        }
    }
}

/// Execute a scheduled update
async fn execute_scheduled_update(
    schedule: &update_scheduler::UpdateSchedule,
) -> Result<(), Box<dyn std::error::Error>> {
    // Get inactive partition
    let inactive = disk::get_inactive_partition()?;

    info!(
        device = %inactive.device,
        source = %schedule.source_url,
        "Starting scheduled update execution"
    );

    // Flash the image
    disk::flash_image(
        &schedule.source_url,
        &inactive.device,
        schedule.expected_sha256.as_deref(),
    )
    .await?;

    // Switch boot partition
    disk::switch_boot_partition(inactive.index)?;

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
