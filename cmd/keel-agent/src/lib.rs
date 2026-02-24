//! KeelOS Agent library
//!
//! Shared types and gRPC service implementation for keel-agent.
//! The library is consumed by the binary entry-point (`main.rs`) and by
//! integration / e2e tests.

// ---- module declarations (public so main.rs and tests can reach them) ----

pub mod cert_metrics;
pub mod cert_renewal;
pub mod disk;
pub mod health;
pub mod health_check;
pub mod hooks;
pub mod k8s_csr;
pub mod mtls;
pub mod network;
pub mod telemetry;
pub mod update_scheduler;

// ---- re-exports for convenience ----

pub use health_check::{HealthChecker, HealthCheckerConfig};
pub use update_scheduler::{ScheduleStatus, UpdateScheduler};

// ---- imports used by the service implementation ----

use keel_api::node::node_service_server::NodeService;
use keel_api::node::{
    BootstrapKubernetesRequest, BootstrapKubernetesResponse, CancelScheduledUpdateRequest,
    CancelScheduledUpdateResponse, ConfigureNetworkRequest, ConfigureNetworkResponse,
    GetBootstrapStatusRequest, GetBootstrapStatusResponse, GetHealthRequest, GetHealthResponse,
    GetNetworkConfigRequest, GetNetworkConfigResponse, GetNetworkStatusRequest,
    GetNetworkStatusResponse, GetRollbackHistoryRequest, GetRollbackHistoryResponse,
    GetStatusRequest, GetStatusResponse, GetUpdateScheduleRequest, GetUpdateScheduleResponse,
    HealthCheckResult as ProtoHealthCheckResult, InitBootstrapRequest, InitBootstrapResponse,
    InstallUpdateRequest, RebootRequest, RebootResponse, RollbackEvent, RotateCertificateRequest,
    RotateCertificateResponse, ScheduleUpdateRequest, ScheduleUpdateResponse,
    TriggerRollbackRequest, TriggerRollbackResponse, UpdateProgress,
    UpdateSchedule as ProtoUpdateSchedule,
};
use std::pin::Pin;
use std::sync::Arc;
use tokio_stream::Stream;
use tonic::{Request, Response, Status};
use tracing::{debug, error, info, warn};

// ---- gRPC service ----

/// gRPC service implementation for `NodeService`.
#[derive(Clone)]
pub struct HelperNodeService {
    /// Shared update scheduler state.
    pub scheduler: Arc<UpdateScheduler>,
    /// Shared health checker state.
    pub health_checker: Arc<HealthChecker>,
}

#[tonic::async_trait]
impl NodeService for HelperNodeService {
    async fn get_status(
        &self,
        _request: Request<GetStatusRequest>,
    ) -> Result<Response<GetStatusResponse>, Status> {
        debug!("Received get_status request");
        let reply = GetStatusResponse {
            hostname: "keel-node".to_string(),    // TODO: Get from hostname
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
                req.is_delta,
                req.fallback_to_full,
                if req.full_image_url.is_empty() {
                    None
                } else {
                    Some(req.full_image_url)
                },
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

    async fn bootstrap_kubernetes(
        &self,
        request: Request<BootstrapKubernetesRequest>,
    ) -> Result<Response<BootstrapKubernetesResponse>, Status> {
        let req = request.into_inner();

        info!(
            api_server = %req.api_server_endpoint,
            has_token = !req.bootstrap_token.is_empty(),
            has_kubeconfig = !req.kubeconfig.is_empty(),
            "Bootstrap Kubernetes request received"
        );

        // Validate inputs
        if req.api_server_endpoint.is_empty() {
            return Err(Status::invalid_argument("api_server_endpoint is required"));
        }

        // Must provide either token+CA or kubeconfig
        if req.bootstrap_token.is_empty() && req.kubeconfig.is_empty() {
            return Err(Status::invalid_argument(
                "Either bootstrap_token or kubeconfig must be provided",
            ));
        }

        // If using token auth, CA cert is required
        if !req.bootstrap_token.is_empty() && req.ca_cert_pem.is_empty() {
            return Err(Status::invalid_argument(
                "ca_cert_pem is required when using bootstrap_token",
            ));
        }

        // Determine node name
        let node_name = if !req.node_name.is_empty() {
            req.node_name.clone()
        } else {
            // Use hostname
            hostname::get()
                .map_err(|e| Status::internal(format!("Failed to get hostname: {}", e)))?
                .to_string_lossy()
                .to_string()
        };

        // Prepare Kubernetes directory
        let base_path = "/var/lib/keel";
        keel_config::bootstrap::prepare_k8s_directories(base_path)
            .map_err(|e| Status::internal(format!("Failed to create directories: {}", e)))?;

        let k8s_dir = format!("{}/kubernetes", base_path);
        let ca_cert_path = format!("{}/ca.crt", k8s_dir);
        let kubeconfig_path = format!("{}/kubelet.kubeconfig", k8s_dir);

        // Write CA certificate
        if !req.ca_cert_pem.is_empty() {
            std::fs::write(&ca_cert_path, &req.ca_cert_pem)
                .map_err(|e| Status::internal(format!("Failed to write CA certificate: {}", e)))?;
            info!(path = %ca_cert_path, "CA certificate written");
        }

        // Generate or write kubeconfig
        let kubeconfig_content = if !req.kubeconfig.is_empty() {
            // Use provided kubeconfig
            String::from_utf8(req.kubeconfig).map_err(|e| {
                Status::invalid_argument(format!("Invalid kubeconfig encoding: {}", e))
            })?
        } else {
            // Generate kubeconfig from token
            keel_config::bootstrap::generate_kubeconfig(
                &req.api_server_endpoint,
                &req.ca_cert_pem,
                &req.bootstrap_token,
                &node_name,
            )
            .map_err(|e| Status::internal(format!("Failed to generate kubeconfig: {}", e)))?
        };

        // Write kubeconfig
        std::fs::write(&kubeconfig_path, &kubeconfig_content)
            .map_err(|e| Status::internal(format!("Failed to write kubeconfig: {}", e)))?;
        info!(path = %kubeconfig_path, "Kubeconfig written");

        // Persist bootstrap configuration
        let bootstrap_config = keel_config::bootstrap::BootstrapConfig::new(
            req.api_server_endpoint.clone(),
            node_name.clone(),
            kubeconfig_path.clone(),
            ca_cert_path.clone(),
        );

        let bootstrap_state_path = format!("{}/bootstrap.json", k8s_dir);
        bootstrap_config
            .save(&bootstrap_state_path)
            .map_err(|e| Status::internal(format!("Failed to save bootstrap state: {}", e)))?;

        // Signal kubelet restart
        let restart_signal_path = "/run/keel/restart-kubelet";
        std::fs::create_dir_all("/run/keel").ok();
        std::fs::write(restart_signal_path, "1")
            .map_err(|e| Status::internal(format!("Failed to create restart signal: {}", e)))?;

        info!(
            node_name = %node_name,
            api_server = %req.api_server_endpoint,
            "Kubernetes bootstrap completed"
        );

        Ok(Response::new(BootstrapKubernetesResponse {
            success: true,
            message: format!(
                "Bootstrap successful. Node '{}' configured to join cluster. Kubelet will restart.",
                node_name
            ),
            kubeconfig_path,
        }))
    }

    async fn get_bootstrap_status(
        &self,
        _request: Request<GetBootstrapStatusRequest>,
    ) -> Result<Response<GetBootstrapStatusResponse>, Status> {
        debug!("Get bootstrap status requested");

        let base_path = "/var/lib/keel";
        let k8s_dir = format!("{}/kubernetes", base_path);
        let bootstrap_state_path = format!("{}/bootstrap.json", k8s_dir);

        // Check if bootstrapped
        if !keel_config::bootstrap::BootstrapConfig::is_bootstrapped(&bootstrap_state_path) {
            return Ok(Response::new(GetBootstrapStatusResponse {
                is_bootstrapped: false,
                api_server_endpoint: String::new(),
                node_name: String::new(),
                kubeconfig_path: String::new(),
                bootstrapped_at: String::new(),
            }));
        }

        // Load bootstrap configuration
        let config =
            keel_config::bootstrap::BootstrapConfig::load(&bootstrap_state_path).map_err(|e| {
                Status::internal(format!("Failed to load bootstrap configuration: {}", e))
            })?;

        Ok(Response::new(GetBootstrapStatusResponse {
            is_bootstrapped: true,
            api_server_endpoint: config.api_server,
            node_name: config.node_name,
            kubeconfig_path: config.kubeconfig_path,
            bootstrapped_at: config.bootstrapped_at,
        }))
    }

    async fn init_bootstrap(
        &self,
        request: Request<InitBootstrapRequest>,
    ) -> Result<Response<InitBootstrapResponse>, Status> {
        let req = request.into_inner();

        info!("Received bootstrap certificate initialization request");

        // Validate the client certificate PEM
        if let Err(e) = keel_crypto::validate_bootstrap_cert(&req.client_cert_pem) {
            warn!("Invalid bootstrap certificate: {}", e);
            return Ok(Response::new(InitBootstrapResponse {
                success: false,
                message: format!("Invalid certificate: {}", e),
            }));
        }

        // Store the client's public certificate in trusted clients directory
        let cert_dir = std::path::Path::new("/var/lib/keel/crypto/trusted-clients/bootstrap");
        if let Err(e) = std::fs::create_dir_all(cert_dir) {
            error!("Failed to create cert directory: {}", e);
            return Ok(Response::new(InitBootstrapResponse {
                success: false,
                message: format!("Failed to create cert directory: {}", e),
            }));
        }

        // Generate a unique filename based on cert hash
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        req.client_cert_pem.hash(&mut hasher);
        let cert_hash = hasher.finish();

        let cert_path = cert_dir.join(format!("client-{:x}.pem", cert_hash));

        if let Err(e) = std::fs::write(&cert_path, &req.client_cert_pem) {
            error!("Failed to write certificate: {}", e);
            return Ok(Response::new(InitBootstrapResponse {
                success: false,
                message: format!("Failed to write certificate: {}", e),
            }));
        }

        info!("Stored bootstrap certificate: {}", cert_path.display());

        Ok(Response::new(InitBootstrapResponse {
            success: true,
            message: "Bootstrap certificate accepted and stored".to_string(),
        }))
    }

    async fn rotate_certificate(
        &self,
        request: Request<RotateCertificateRequest>,
    ) -> Result<Response<RotateCertificateResponse>, Status> {
        use k8s_csr::K8sCsrManager;

        let req = request.into_inner();
        info!("Certificate rotation requested (force: {})", req.force);

        // Check if running in K8s
        if !std::path::Path::new("/var/run/secrets/kubernetes.io/serviceaccount/token").exists() {
            return Err(Status::failed_precondition(
                "Not running in Kubernetes - rotation only available in K8s clusters",
            ));
        }

        // Get node name
        let node_name = std::env::var("NODE_NAME")
            .ok()
            .or_else(|| hostname::get().ok().and_then(|h| h.into_string().ok()))
            .ok_or_else(|| Status::internal("Failed to determine node name"))?;

        info!("Rotating certificate for node: {}", node_name);

        // Create K8s CSR manager and request new certificate
        let csr_manager = K8sCsrManager::new(node_name)
            .await
            .map_err(|e| Status::internal(format!("Failed to initialize CSR manager: {}", e)))?;

        match csr_manager.request_certificate().await {
            Ok((cert_pem, key_pem)) => {
                // Store new certificates
                let cert_path = "/var/lib/keel/crypto/operational.pem";
                let key_path = "/var/lib/keel/crypto/operational.key";

                // Create backup of old certificates
                if std::path::Path::new(cert_path).exists() {
                    let backup_cert = format!("{}.backup", cert_path);
                    let backup_key = format!("{}.backup", key_path);
                    if let Err(e) = std::fs::copy(cert_path, &backup_cert) {
                        warn!("Failed to backup old certificate: {}", e);
                    }
                    if let Err(e) = std::fs::copy(key_path, &backup_key) {
                        warn!("Failed to backup old key: {}", e);
                    }
                    info!("Backed up old certificates");
                }

                // Write new certificates
                if let Err(e) = std::fs::write(cert_path, &cert_pem) {
                    return Err(Status::internal(format!(
                        "Failed to write new certificate: {}",
                        e
                    )));
                }

                if let Err(e) = std::fs::write(key_path, &key_pem) {
                    return Err(Status::internal(format!("Failed to write new key: {}", e)));
                }

                info!("✓ Certificate rotation successful");

                // Parse actual expiry from certificate
                let expires_at = keel_crypto::parse_cert_expiry(&cert_pem)
                    .map(|dt| dt.to_rfc3339())
                    .unwrap_or_else(|e| {
                        warn!("Failed to parse certificate expiry: {}", e);
                        "Unknown".to_string()
                    });

                Ok(Response::new(RotateCertificateResponse {
                    success: true,
                    message: "Certificate rotated successfully".to_string(),
                    cert_path: cert_path.to_string(),
                    expires_at,
                }))
            }
            Err(e) => {
                error!("Certificate rotation failed: {}", e);
                Err(Status::internal(format!("Rotation failed: {}", e)))
            }
        }
    }

    async fn configure_network(
        &self,
        request: Request<ConfigureNetworkRequest>,
    ) -> Result<Response<ConfigureNetworkResponse>, Status> {
        network::configure_network(request).await
    }

    async fn get_network_config(
        &self,
        request: Request<GetNetworkConfigRequest>,
    ) -> Result<Response<GetNetworkConfigResponse>, Status> {
        network::get_network_config(request).await
    }

    async fn get_network_status(
        &self,
        request: Request<GetNetworkStatusRequest>,
    ) -> Result<Response<GetNetworkStatusResponse>, Status> {
        network::get_network_status(request).await
    }
}
