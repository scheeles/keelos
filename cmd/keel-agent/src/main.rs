//! KeelOS Agent - gRPC management server
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

use keel_api::node::node_service_server::{NodeService, NodeServiceServer};
use keel_api::node::{
    CancelScheduledUpdateRequest, CancelScheduledUpdateResponse, GetHealthRequest,
    GetHealthResponse, GetRollbackHistoryRequest, GetRollbackHistoryResponse, GetStatusRequest,
    GetStatusResponse, GetUpdateScheduleRequest, GetUpdateScheduleResponse,
    HealthCheckResult as ProtoHealthCheckResult, InstallUpdateRequest, RebootRequest,
    RebootResponse, RollbackEvent, ScheduleUpdateRequest, ScheduleUpdateResponse,
    TriggerRollbackRequest, TriggerRollbackResponse, UpdateProgress,
    UpdateSchedule as ProtoUpdateSchedule,
};
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio_stream::Stream;
use tonic::{transport::Server, Request, Response, Status};
use tracing::{debug, error, info, warn};

mod disk;
mod health;
mod health_check;
mod telemetry;
mod update_scheduler;

use health_check::{HealthChecker, HealthCheckerConfig};
use update_scheduler::{ScheduleStatus, UpdateScheduler};

#[derive(Clone)]
pub struct HelperNodeService {
    scheduler: Arc<UpdateScheduler>,
    health_checker: Arc<HealthChecker>,
}

#[tonic::async_trait]
impl NodeService for HelperNodeService {
    async fn get_status(
        &self,
        _request: Request<GetStatusRequest>,
    ) -> Result<Response<GetStatusResponse>, Status> {
        debug!("Received get_status request");
        let reply = GetStatusResponse {
            hostname: "keel-node".to_string(),   // TODO: Get from hostname
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
        let is_delta = req.is_delta;
        let fallback_url = if req.fallback_to_full && !req.full_image_url.is_empty() {
            Some(req.full_image_url.clone())
        } else {
            None
        };
        
        info!(
            source = %source_url, 
            has_sha256 = expected_sha256.is_some(),
            is_delta = is_delta,
            has_fallback = fallback_url.is_some(),
            "Install update requested"
        );

        let output = async_stream::try_stream! {
            yield UpdateProgress {
                percentage: 0,
                message: "Identifying target partition...".to_string(),
                success: false,
                download_speed_bps: 0,
                eta_seconds: 0,
                phase: "preparing".to_string(),
                bytes_saved: 0,
            };

            let inactive = disk::get_inactive_partition()
                .map_err(|e| Status::internal(format!("Failed to get inactive partition: {}", e)))?;

            debug!(device = %inactive.device, index = inactive.index, "Identified inactive partition");

            yield UpdateProgress {
                percentage: 10,
                message: format!("Target partition identified: {}", inactive.device),
                success: false,
                download_speed_bps: 0,
                eta_seconds: 0,
                phase: "preparing".to_string(),
                bytes_saved: 0,
            };

            let phase_msg = if is_delta {
                format!("Downloading delta and patching to {}...", inactive.device)
            } else {
                format!("Downloading and flashing to {}...", inactive.device)
            };

            yield UpdateProgress {
                percentage: 20,
                message: phase_msg,
                success: false,
                download_speed_bps: 0,
                eta_seconds: 0,
                phase: "downloading".to_string(),
                bytes_saved: 0,
            };

            // Disk flashing with delta support
            let bytes_saved = disk::flash_image(
                &source_url,
                &inactive.device,
                expected_sha256.as_deref(),
                is_delta,
                fallback_url.as_deref(),
            ).await
                .map_err(|e| Status::internal(format!("Flash error: {}", e)))?;

            if is_delta && bytes_saved > 0 {
                info!(bytes_saved = bytes_saved, "Delta update saved bandwidth");
            }

            yield UpdateProgress {
                percentage: 80,
                message: "Image flashed. Toggling boot flags...".to_string(),
                success: false,
                download_speed_bps: 0,
                eta_seconds: 0,
                phase: "verifying".to_string(),
                bytes_saved,
            };

            disk::switch_boot_partition(inactive.index)
                .map_err(|e| Status::internal(format!("Failed to switch boot partition: {}", e)))?;

            info!(target_partition = inactive.index, "Update installed successfully");

            let final_msg = if bytes_saved > 0 {
                format!("Update installed successfully. Saved {} bytes. Reboot to apply.", bytes_saved)
            } else {
                "Update installed successfully. Reboot to apply.".to_string()
            };

            yield UpdateProgress {
                percentage: 100,
                message: final_msg,
                success: true,
                download_speed_bps: 0,
                eta_seconds: 0,
                phase: "completed".to_string(),
                bytes_saved,
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

        let health_check_timeout = if req.health_check_timeout_secs > 0 {
            Some(req.health_check_timeout_secs)
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
                health_check_timeout,
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

    async fn get_health(
        &self,
        _request: Request<GetHealthRequest>,
    ) -> Result<Response<GetHealthResponse>, Status> {
        debug!("Get health requested");

        let (status, executions) = self.health_checker.run_all_checks().await;

        let proto_checks: Vec<ProtoHealthCheckResult> = executions
            .into_iter()
            .map(|exec| ProtoHealthCheckResult {
                name: exec.name,
                status: match exec.result {
                    health_check::HealthCheckResult::Pass => "pass".to_string(),
                    health_check::HealthCheckResult::Fail(_) => "fail".to_string(),
                    health_check::HealthCheckResult::Unknown(_) => "unknown".to_string(),
                },
                message: exec.result.message(),
                duration_ms: exec.duration_ms,
            })
            .collect();

        Ok(Response::new(GetHealthResponse {
            status: status.to_string(),
            checks: proto_checks,
            last_update_time: chrono::Utc::now().to_rfc3339(),
        }))
    }

    async fn trigger_rollback(
        &self,
        request: Request<TriggerRollbackRequest>,
    ) -> Result<Response<TriggerRollbackResponse>, Status> {
        let reason = request.into_inner().reason;

        info!(reason = %reason, "Manual rollback requested");

        // Perform rollback
        match disk::rollback_to_previous_partition() {
            Ok(_) => {
                info!("Rollback completed successfully");
                Ok(Response::new(TriggerRollbackResponse {
                    success: true,
                    message: "Rollback completed. System will reboot to previous partition."
                        .to_string(),
                }))
            }
            Err(e) => {
                warn!(error = %e, "Rollback failed");
                Ok(Response::new(TriggerRollbackResponse {
                    success: false,
                    message: format!("Rollback failed: {}", e),
                }))
            }
        }
    }

    async fn get_rollback_history(
        &self,
        _request: Request<GetRollbackHistoryRequest>,
    ) -> Result<Response<GetRollbackHistoryResponse>, Status> {
        debug!("Get rollback history requested");

        // Get all schedules that have been rolled back
        let schedules = self.scheduler.get_schedules().await;

        let rollback_events: Vec<RollbackEvent> = schedules
            .into_iter()
            .filter(|s| s.rollback_triggered)
            .map(|s| RollbackEvent {
                timestamp: s.completed_at.map(|dt| dt.to_rfc3339()).unwrap_or_default(),
                reason: s.rollback_reason.unwrap_or_else(|| "Unknown".to_string()),
                from_partition: "unknown".to_string(), // TODO: Track actual partition info
                to_partition: "unknown".to_string(),   // TODO: Track actual partition info
                automatic: s.enable_auto_rollback,
            })
            .collect();

        info!(count = rollback_events.len(), "Retrieved rollback history");

        Ok(Response::new(GetRollbackHistoryResponse {
            events: rollback_events,
        }))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize OpenTelemetry telemetry
    let otlp_endpoint = std::env::var("OTLP_ENDPOINT").ok();
    telemetry::init_telemetry("keel-agent", otlp_endpoint)?;

    let grpc_addr: std::net::SocketAddr = "0.0.0.0:50051".parse()?;
    let health_addr: std::net::SocketAddr = "0.0.0.0:9090".parse()?;

    // Initialize update scheduler
    let scheduler = Arc::new(UpdateScheduler::new("/var/lib/keel/update-schedule.json"));

    // Initialize health checker
    let health_config = HealthCheckerConfig::default();
    let health_checker = Arc::new(HealthChecker::new(health_config));

    // Start background executor for scheduled updates
    let executor_scheduler = scheduler.clone();
    tokio::spawn(async move {
        schedule_executor(executor_scheduler).await;
    });

    let node_service = HelperNodeService {
        scheduler: scheduler.clone(),
        health_checker: health_checker.clone(),
    };

    info!(grpc_addr = %grpc_addr, "Matic Agent starting");

    // Load declarative configuration
    let config_path = "/etc/keel/node.yaml";
    let config = if std::path::Path::new(config_path).exists() {
        info!(path = config_path, "Loading configuration");
        keel_config::NodeConfig::load(config_path)?
    } else {
        warn!(
            path = config_path,
            "Configuration not found, using defaults"
        );
        keel_config::NodeConfig::default_config()
    };
    info!(hostname = %config.hostname, "Configuration loaded");

    // mTLS setup
    let cert_path = "/etc/keel/crypto/server.pem";
    let key_path = "/etc/keel/crypto/server.key";
    let ca_path = "/etc/keel/crypto/ca.pem";

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
                Err(error_msg) => {
                    error!(schedule_id = %schedule.id, error = %error_msg, "Scheduled update failed");
                    let _ = scheduler
                        .update_status(&schedule.id, ScheduleStatus::Failed, Some(error_msg))
                        .await;
                }
            }
        }
    }
}

/// Execute a scheduled update
async fn execute_scheduled_update(
    schedule: &update_scheduler::UpdateSchedule,
) -> Result<(), String> {
    // Get inactive partition
    let inactive = disk::get_inactive_partition().map_err(|e| e.to_string())?;

    info!(
        device = %inactive.device,
        source = %schedule.source_url,
        "Starting scheduled update execution"
    );

    // Flash the image (delta support will be added to UpdateSchedule in future)
    disk::flash_image(
        &schedule.source_url,
        &inactive.device,
        schedule.expected_sha256.as_deref(),
        false, // is_delta - TODO: add to UpdateSchedule struct
        None,  // fallback_url - TODO: add to UpdateSchedule struct
    )
    .await
    .map_err(|e| e.to_string())?;

    // Switch boot partition
    disk::switch_boot_partition(inactive.index).map_err(|e| e.to_string())?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use keel_api::node::node_service_server::NodeService;
    use keel_api::node::GetStatusRequest;

    #[tokio::test]
    async fn test_get_status() {
        let scheduler = Arc::new(UpdateScheduler::new("/tmp/test-schedules.json"));
        let health_checker = Arc::new(HealthChecker::new(HealthCheckerConfig::default()));
        let service = HelperNodeService {
            scheduler,
            health_checker,
        };
        let request = tonic::Request::new(GetStatusRequest {});
        let response = service.get_status(request).await.unwrap();
        let inner = response.into_inner();

        assert_eq!(inner.hostname, "keel-node");
        assert_eq!(inner.os_version, "0.1.0");
    }
}
