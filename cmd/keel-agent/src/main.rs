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
    AnalyzeCrashDumpRequest, AnalyzeCrashDumpResponse, BootstrapKubernetesRequest,
    BootstrapKubernetesResponse, CancelScheduledUpdateRequest, CancelScheduledUpdateResponse,
    CollectCrashDumpRequest, CollectCrashDumpResponse, ConfigureNetworkRequest,
    ConfigureNetworkResponse, CrashDumpFinding as ProtoCrashDumpFinding,
    CreateSystemSnapshotRequest, CreateSystemSnapshotResponse, EnableDebugModeRequest,
    EnableDebugModeResponse, EnableRecoveryModeRequest, EnableRecoveryModeResponse,
    GetBootstrapStatusRequest, GetBootstrapStatusResponse, GetDebugStatusRequest,
    GetDebugStatusResponse, GetHealthRequest, GetHealthResponse, GetNetworkConfigRequest,
    GetNetworkConfigResponse, GetNetworkStatusRequest, GetNetworkStatusResponse,
    GetRollbackHistoryRequest, GetRollbackHistoryResponse, GetStatusRequest, GetStatusResponse,
    GetUpdateScheduleRequest, GetUpdateScheduleResponse,
    HealthCheckResult as ProtoHealthCheckResult, InitBootstrapRequest, InitBootstrapResponse,
    InstallUpdateRequest, LogEntry, RebootRequest, RebootResponse, RollbackEvent,
    RotateCertificateRequest, RotateCertificateResponse, ScheduleUpdateRequest,
    ScheduleUpdateResponse, StreamLogsRequest, TriggerRollbackRequest, TriggerRollbackResponse,
    UpdateProgress, UpdateSchedule as ProtoUpdateSchedule,
};
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio_stream::Stream;
use tonic::{transport::Server, Request, Response, Status};
use tracing::{debug, error, info, warn};

mod cert_metrics;
mod cert_renewal;
mod diagnostics;
mod disk;
mod health;
mod health_check;
mod hooks;
mod k8s_csr;
mod mtls;
mod network;
mod telemetry;
mod update_scheduler;

use diagnostics::DiagnosticsManager;
use health_check::{HealthChecker, HealthCheckerConfig};
use hooks::execute_hook;
use mtls::TlsManager;
use update_scheduler::{ScheduleStatus, UpdateScheduler};

#[derive(Clone)]
pub struct HelperNodeService {
    scheduler: Arc<UpdateScheduler>,
    health_checker: Arc<HealthChecker>,
    diagnostics: Arc<DiagnosticsManager>,
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

    async fn enable_debug_mode(
        &self,
        request: Request<EnableDebugModeRequest>,
    ) -> Result<Response<EnableDebugModeResponse>, Status> {
        let req = request.into_inner();

        info!(
            reason = %req.reason,
            duration_secs = req.duration_secs,
            "Enable debug mode requested (audit)"
        );

        match self
            .diagnostics
            .enable_debug_mode(req.duration_secs, &req.reason)
            .await
        {
            Ok(session) => Ok(Response::new(EnableDebugModeResponse {
                success: true,
                message: "Debug mode enabled".to_string(),
                session_id: session.session_id,
                expires_at: session.expires_at.to_rfc3339(),
            })),
            Err(e) => Ok(Response::new(EnableDebugModeResponse {
                success: false,
                message: e,
                session_id: String::new(),
                expires_at: String::new(),
            })),
        }
    }

    async fn get_debug_status(
        &self,
        _request: Request<GetDebugStatusRequest>,
    ) -> Result<Response<GetDebugStatusResponse>, Status> {
        debug!("Get debug status requested");

        match self.diagnostics.get_debug_status().await {
            Some(session) => {
                let remaining = (session.expires_at - chrono::Utc::now()).num_seconds();
                let remaining = if remaining > 0 { remaining as u32 } else { 0 };
                Ok(Response::new(GetDebugStatusResponse {
                    enabled: true,
                    session_id: session.session_id,
                    expires_at: session.expires_at.to_rfc3339(),
                    reason: session.reason,
                    remaining_secs: remaining,
                }))
            }
            None => Ok(Response::new(GetDebugStatusResponse {
                enabled: false,
                session_id: String::new(),
                expires_at: String::new(),
                reason: String::new(),
                remaining_secs: 0,
            })),
        }
    }

    async fn collect_crash_dump(
        &self,
        request: Request<CollectCrashDumpRequest>,
    ) -> Result<Response<CollectCrashDumpResponse>, Status> {
        let req = request.into_inner();

        info!(
            include_kernel = req.include_kernel,
            include_userspace = req.include_userspace,
            "Crash dump collection requested"
        );

        match diagnostics::collect_crash_dump(req.include_kernel, req.include_userspace) {
            Ok((dump_path, dump_size)) => Ok(Response::new(CollectCrashDumpResponse {
                success: true,
                message: "Crash dump collected successfully".to_string(),
                dump_path,
                dump_size_bytes: dump_size,
                created_at: chrono::Utc::now().to_rfc3339(),
            })),
            Err(e) => Ok(Response::new(CollectCrashDumpResponse {
                success: false,
                message: format!("Failed to collect crash dump: {e}"),
                dump_path: String::new(),
                dump_size_bytes: 0,
                created_at: String::new(),
            })),
        }
    }

    type StreamLogsStream = Pin<Box<dyn Stream<Item = Result<LogEntry, Status>> + Send>>;

    async fn stream_logs(
        &self,
        request: Request<StreamLogsRequest>,
    ) -> Result<Response<Self::StreamLogsStream>, Status> {
        let req = request.into_inner();
        let level_filter = req.level.clone();
        let component_filter = req.component.clone();
        let tail_lines = if req.tail_lines == 0 {
            50
        } else {
            req.tail_lines
        };

        info!(
            level = %level_filter,
            component = %component_filter,
            tail_lines = tail_lines,
            "Log streaming requested"
        );

        // Collect log lines synchronously, then stream them
        let dmesg_output = std::process::Command::new("dmesg")
            .arg("--time-format=iso")
            .output()
            .map_err(|e| Status::internal(format!("Failed to read dmesg: {e}")))?;

        let full_output = String::from_utf8_lossy(&dmesg_output.stdout).to_string();
        let all_lines: Vec<String> = full_output.lines().map(String::from).collect();
        let start = if all_lines.len() > tail_lines as usize {
            all_lines.len() - tail_lines as usize
        } else {
            0
        };
        let selected_lines: Vec<String> = all_lines[start..].to_vec();

        let output = async_stream::try_stream! {
            for line in &selected_lines {
                let entry = parse_log_line(line, &level_filter, &component_filter);
                if let Some(entry) = entry {
                    yield entry;
                }
            }
        };

        Ok(Response::new(Box::pin(output) as Self::StreamLogsStream))
    }

    async fn create_system_snapshot(
        &self,
        request: Request<CreateSystemSnapshotRequest>,
    ) -> Result<Response<CreateSystemSnapshotResponse>, Status> {
        let req = request.into_inner();

        info!(
            label = %req.label,
            include_config = req.include_config,
            include_logs = req.include_logs,
            "System snapshot requested"
        );

        match diagnostics::create_system_snapshot(&req.label, req.include_config, req.include_logs)
        {
            Ok((snapshot_id, snapshot_path, size)) => {
                Ok(Response::new(CreateSystemSnapshotResponse {
                    success: true,
                    message: "System snapshot created successfully".to_string(),
                    snapshot_id,
                    snapshot_path,
                    size_bytes: size,
                    created_at: chrono::Utc::now().to_rfc3339(),
                }))
            }
            Err(e) => Ok(Response::new(CreateSystemSnapshotResponse {
                success: false,
                message: format!("Failed to create snapshot: {e}"),
                snapshot_id: String::new(),
                snapshot_path: String::new(),
                size_bytes: 0,
                created_at: String::new(),
            })),
        }
    }

    async fn enable_recovery_mode(
        &self,
        request: Request<EnableRecoveryModeRequest>,
    ) -> Result<Response<EnableRecoveryModeResponse>, Status> {
        let req = request.into_inner();

        info!(
            reason = %req.reason,
            duration_secs = req.duration_secs,
            "Enable recovery mode requested (audit)"
        );

        match self
            .diagnostics
            .enable_recovery_mode(req.duration_secs, &req.reason)
            .await
        {
            Ok(session) => Ok(Response::new(EnableRecoveryModeResponse {
                success: true,
                message: format!("Recovery mode enabled (reason: {})", session.reason),
                expires_at: session.expires_at.to_rfc3339(),
            })),
            Err(e) => Ok(Response::new(EnableRecoveryModeResponse {
                success: false,
                message: e,
                expires_at: String::new(),
            })),
        }
    }

    async fn analyze_crash_dump(
        &self,
        request: Request<AnalyzeCrashDumpRequest>,
    ) -> Result<Response<AnalyzeCrashDumpResponse>, Status> {
        let req = request.into_inner();

        info!(dump_path = %req.dump_path, "Crash dump analysis requested");

        match diagnostics::analyze_crash_dump(&req.dump_path) {
            Ok((severity, findings, summary)) => {
                let proto_findings = findings
                    .into_iter()
                    .map(|f| ProtoCrashDumpFinding {
                        severity: f.severity.to_string(),
                        finding_type: f.finding_type.to_string(),
                        message: f.message,
                    })
                    .collect();
                Ok(Response::new(AnalyzeCrashDumpResponse {
                    success: true,
                    message: "Crash dump analyzed successfully".to_string(),
                    severity,
                    findings: proto_findings,
                    summary,
                }))
            }
            Err(e) => Ok(Response::new(AnalyzeCrashDumpResponse {
                success: false,
                message: format!("Failed to analyze crash dump: {e}"),
                severity: String::new(),
                findings: vec![],
                summary: String::new(),
            })),
        }
    }
}

/// Parse a raw log line and apply filters.
/// Returns `None` if the line doesn't match filters.
fn parse_log_line(line: &str, level_filter: &str, component_filter: &str) -> Option<LogEntry> {
    // dmesg --time-format=iso lines look like:
    //   "2024-01-01T00:00:00,000000+00:00 kern.warn: something happened"
    // Try to extract timestamp and level from the line
    let (timestamp, level, message) = if let Some((ts, rest)) = line.split_once(' ') {
        // Try to extract facility.level prefix (e.g. "kern.warn:")
        if let Some((facility_level, msg)) = rest.split_once(": ") {
            let level = if let Some((_, lvl)) = facility_level.split_once('.') {
                match lvl {
                    "emerg" | "alert" | "crit" | "err" => "error",
                    "warn" | "warning" => "warn",
                    "notice" | "info" => "info",
                    "debug" => "debug",
                    _ => "info",
                }
            } else {
                "info"
            };
            (ts.to_string(), level.to_string(), msg.to_string())
        } else {
            (ts.to_string(), "info".to_string(), rest.to_string())
        }
    } else {
        (
            chrono::Utc::now().to_rfc3339(),
            "info".to_string(),
            line.to_string(),
        )
    };

    // Apply level filter if specified
    if !level_filter.is_empty() && level != level_filter {
        return None;
    }

    // Apply component filter if specified (kernel logs always have component "kernel")
    let component = "kernel".to_string();
    if !component_filter.is_empty() && component != component_filter {
        return None;
    }

    Some(LogEntry {
        timestamp,
        level,
        component,
        message,
    })
}

/// Initialize operational certificates if running in Kubernetes
/// Returns (cert_path, key_path) if successful, None if not in K8s or on error
async fn init_k8s_certificates() -> Option<(String, String)> {
    use k8s_csr::K8sCsrManager;

    // Check if we're running in Kubernetes
    if !std::path::Path::new("/var/run/secrets/kubernetes.io/serviceaccount/token").exists() {
        info!("Not running in Kubernetes, skipping operational certificate initialization");
        return None;
    }

    // Get node name from environment or hostname
    let node_name = std::env::var("NODE_NAME")
        .ok()
        .or_else(|| hostname::get().ok().and_then(|h| h.into_string().ok()))?;

    info!(
        "Initializing operational certificates for node: {}",
        node_name
    );

    // Check if we already have valid operational certificates
    let cert_path = "/var/lib/keel/crypto/operational.pem";
    let key_path = "/var/lib/keel/crypto/operational.key";

    if std::path::Path::new(cert_path).exists() && std::path::Path::new(key_path).exists() {
        // TODO: Check certificate expiry and renew if needed
        info!("Operational certificates already exist, using existing certs");
        return Some((cert_path.to_string(), key_path.to_string()));
    }

    // Create K8s CSR manager and request certificate
    match K8sCsrManager::new(node_name).await {
        Ok(csr_manager) => {
            info!("Requesting operational certificate from Kubernetes...");

            match csr_manager.request_certificate().await {
                Ok((cert_pem, key_pem)) => {
                    // Store the certificates
                    if let Err(e) = std::fs::create_dir_all("/var/lib/keel/crypto") {
                        warn!("Failed to create crypto directory: {}", e);
                        return None;
                    }

                    if let Err(e) = std::fs::write(cert_path, &cert_pem) {
                        warn!("Failed to write operational certificate: {}", e);
                        return None;
                    }

                    if let Err(e) = std::fs::write(key_path, &key_pem) {
                        warn!("Failed to write operational key: {}", e);
                        return None;
                    }

                    info!("✓ Successfully obtained and stored operational certificates");
                    Some((cert_path.to_string(), key_path.to_string()))
                }
                Err(e) => {
                    warn!("Failed to request operational certificate: {}", e);
                    info!("Agent will use bootstrap certificates if available");
                    None
                }
            }
        }
        Err(e) => {
            warn!("Failed to initialize K8s CSR manager: {}", e);
            None
        }
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

    // Initialize diagnostics manager
    let diagnostics = Arc::new(DiagnosticsManager::new());

    // Start background executor for scheduled updates
    let executor_scheduler = scheduler.clone();
    tokio::spawn(async move {
        schedule_executor(executor_scheduler).await;
    });

    let node_service = HelperNodeService {
        scheduler: scheduler.clone(),
        health_checker: health_checker.clone(),
        diagnostics,
    };

    info!(grpc_addr = %grpc_addr, "Matic Agent starting");

    // Initialize K8s operational certificates if running in cluster
    if let Some((cert_path, key_path)) = init_k8s_certificates().await {
        info!("K8s operational certificates initialized:");
        info!("  Cert: {}", cert_path);
        info!("  Key: {}", key_path);
    }

    // Start certificate auto-renewal daemon if operational cert exists
    let operational_cert_path = "/var/lib/keel/crypto/operational.pem";
    if std::path::Path::new(operational_cert_path).exists() {
        use cert_renewal::{CertRenewalConfig, CertRenewalManager};

        let renewal_config = CertRenewalConfig {
            operational_cert_path: operational_cert_path.to_string(),
            operational_key_path: "/var/lib/keel/crypto/operational.key".to_string(),
            renewal_threshold_days: 30, // Renew 30 days before expiry
            check_interval_hours: 24,   // Check once per day
        };

        let renewal_manager = Arc::new(CertRenewalManager::new(renewal_config));

        tokio::spawn(async move {
            renewal_manager.start_renewal_loop().await;
        });

        info!("Certificate auto-renewal enabled (threshold: 30 days, check interval: 24 hours)");
    }

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

    // mTLS setup with dual-CA support
    // Supports both bootstrap (self-signed) and operational (K8s-signed) certificates
    let server_cert_path = "/etc/keel/crypto/server.pem";
    let server_key_path = "/etc/keel/crypto/server.key";
    let bootstrap_ca_dir = "/var/lib/keel/crypto/trusted-clients/bootstrap";
    let operational_ca_path = "/etc/keel/crypto/ca.pem";

    let mut builder = Server::builder()
        .http2_keepalive_interval(Some(std::time::Duration::from_secs(10)))
        .http2_keepalive_timeout(Some(std::time::Duration::from_secs(20)))
        .tcp_keepalive(Some(std::time::Duration::from_secs(10)));

    // Try to configure TLS with dual-CA support
    let tls_manager = TlsManager::new(
        server_cert_path.to_string(),
        server_key_path.to_string(),
        bootstrap_ca_dir.to_string(),
        Some(operational_ca_path.to_string()),
    );

    if tls_manager.can_configure() {
        info!("Enabling mTLS with dual-CA support (bootstrap + operational)");
        match tls_manager.build_tls_config() {
            Ok(tls_config) => {
                builder = builder.tls_config(tls_config)?;
                info!("mTLS enabled successfully");
            }
            Err(e) => {
                warn!("Failed to configure TLS: {}. Running without mTLS.", e);
            }
        }
    } else {
        warn!(
            "Server certificates not found at {}. Running without mTLS.",
            server_cert_path
        );
        info!("To enable mTLS, generate server certificate and key.");
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

    // Start rollback supervisor
    let rb_health = health_checker.clone();
    let rb_scheduler = scheduler.clone();
    tokio::spawn(async move {
        start_rollback_supervisor(rb_health, rb_scheduler).await;
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
        is_delta = schedule.is_delta,
        "Starting scheduled update execution"
    );

    // Run Pre-update hook
    if let Some(hook) = &schedule.pre_update_hook {
        execute_hook(hook, "pre-update").await?;
    }

    // Flash the image with stored delta settings
    disk::flash_image(
        &schedule.source_url,
        &inactive.device,
        schedule.expected_sha256.as_deref(),
        schedule.is_delta,
        schedule
            .fallback_to_full
            .then_some(schedule.full_image_url.as_deref())
            .flatten(),
    )
    .await
    .map_err(|e| e.to_string())?;

    // Run Post-update hook
    if let Some(hook) = &schedule.post_update_hook {
        execute_hook(hook, "post-update").await?;
    }

    // Switch boot partition
    disk::switch_boot_partition(inactive.index).map_err(|e| e.to_string())?;

    Ok(())
}

/// Rollback supervisor checks health after boot and triggers rollback if critical
async fn start_rollback_supervisor(health: Arc<HealthChecker>, scheduler: Arc<UpdateScheduler>) {
    use tokio::time::{sleep, Duration};

    // Allow system to stabilize (initial grace period)
    info!("Rollback supervisor started - waiting for system stability (60s)");
    sleep(Duration::from_secs(60)).await;

    // Run critical check
    let (status, _) = health.run_all_checks().await;

    if status == health_check::HealthStatus::Unhealthy {
        error!(status = %status, "Critical health failure detected!");

        // Try to identify if there was a recent update to mark as failed/rolledback
        // In a real scenario, we'd query the scheduler for the last "Completed" update that might be the cause
        // For now, we'll log it at the system level.

        warn!("Initiating AUTOMATIC ROLLBACK due to critical health failure");

        // Attempt to persist rollback state (best effort before reboot)
        // Note: This relies on storage being writable and shared across boots if we want to see it after rollback
        if let Err(e) = scheduler
            .register_rollback("Critical health failure at boot")
            .await
        {
            error!(error = %e, "Failed to persist rollback event");
        }

        match disk::rollback_to_previous_partition() {
            Ok(_) => {
                error!("Rollback successful - rebooting system...");
                // Force reboot (in real system, would involve syscall/init)
                let _ = std::process::Command::new("reboot").status();
            }
            Err(e) => error!(error = %e, "Automatic rollback FAILED"),
        }
    } else {
        info!("System health verified stable.");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use keel_api::node::node_service_server::NodeService;
    use keel_api::node::{
        EnableDebugModeRequest, EnableRecoveryModeRequest, GetDebugStatusRequest, GetStatusRequest,
    };

    fn make_test_service() -> HelperNodeService {
        HelperNodeService {
            scheduler: Arc::new(UpdateScheduler::new("/tmp/test-schedules.json")),
            health_checker: Arc::new(HealthChecker::new(HealthCheckerConfig::default())),
            diagnostics: Arc::new(DiagnosticsManager::new()),
        }
    }

    #[tokio::test]
    async fn test_get_status() {
        let service = make_test_service();
        let request = tonic::Request::new(GetStatusRequest {});
        let response = service.get_status(request).await.unwrap();
        let inner = response.into_inner();

        assert_eq!(inner.hostname, "keel-node");
        assert_eq!(inner.os_version, "0.1.0");
    }

    #[tokio::test]
    async fn test_enable_debug_mode_via_grpc() {
        let service = make_test_service();
        let request = tonic::Request::new(EnableDebugModeRequest {
            duration_secs: 300,
            reason: "testing diagnostics".to_string(),
        });
        let response = service.enable_debug_mode(request).await.unwrap();
        let inner = response.into_inner();

        assert!(inner.success);
        assert!(!inner.session_id.is_empty());
        assert!(!inner.expires_at.is_empty());
    }

    #[tokio::test]
    async fn test_get_debug_status_inactive() {
        let service = make_test_service();
        let request = tonic::Request::new(GetDebugStatusRequest {});
        let response = service.get_debug_status(request).await.unwrap();
        let inner = response.into_inner();

        assert!(!inner.enabled);
        assert!(inner.session_id.is_empty());
    }

    #[tokio::test]
    async fn test_get_debug_status_after_enable() {
        let service = make_test_service();

        // Enable debug mode
        let enable_req = tonic::Request::new(EnableDebugModeRequest {
            duration_secs: 300,
            reason: "test".to_string(),
        });
        let _ = service.enable_debug_mode(enable_req).await.unwrap();

        // Check status
        let status_req = tonic::Request::new(GetDebugStatusRequest {});
        let response = service.get_debug_status(status_req).await.unwrap();
        let inner = response.into_inner();

        assert!(inner.enabled);
        assert!(!inner.session_id.is_empty());
        assert!(inner.remaining_secs > 0);
    }

    #[tokio::test]
    async fn test_enable_recovery_mode_via_grpc() {
        let service = make_test_service();
        let request = tonic::Request::new(EnableRecoveryModeRequest {
            duration_secs: 600,
            reason: "emergency repair".to_string(),
        });
        let response = service.enable_recovery_mode(request).await.unwrap();
        let inner = response.into_inner();

        assert!(inner.success);
        assert!(inner.message.contains("emergency repair"));
        assert!(!inner.expires_at.is_empty());
    }

    #[tokio::test]
    async fn test_enable_recovery_mode_rejects_duplicate_via_grpc() {
        let service = make_test_service();

        let req1 = tonic::Request::new(EnableRecoveryModeRequest {
            duration_secs: 600,
            reason: "first".to_string(),
        });
        let resp1 = service.enable_recovery_mode(req1).await.unwrap();
        assert!(resp1.into_inner().success);

        let req2 = tonic::Request::new(EnableRecoveryModeRequest {
            duration_secs: 600,
            reason: "second".to_string(),
        });
        let resp2 = service.enable_recovery_mode(req2).await.unwrap();
        assert!(!resp2.into_inner().success);
    }

    #[test]
    fn test_parse_log_line_basic() {
        let entry = parse_log_line("2024-01-01T00:00:00 kern.info: test message", "", "");
        assert!(entry.is_some());
        let entry = entry.unwrap();
        assert_eq!(entry.level, "info");
        assert_eq!(entry.component, "kernel");
        assert_eq!(entry.message, "test message");
    }

    #[test]
    fn test_parse_log_line_error_level() {
        let entry = parse_log_line("2024-01-01T00:00:00 kern.err: something failed", "", "");
        assert!(entry.is_some());
        let entry = entry.unwrap();
        assert_eq!(entry.level, "error");
    }

    #[test]
    fn test_parse_log_line_warn_level() {
        let entry = parse_log_line("2024-01-01T00:00:00 kern.warn: low memory", "", "");
        assert!(entry.is_some());
        let entry = entry.unwrap();
        assert_eq!(entry.level, "warn");
    }

    #[test]
    fn test_parse_log_line_debug_level() {
        let entry = parse_log_line("2024-01-01T00:00:00 kern.debug: verbose info", "", "");
        assert!(entry.is_some());
        let entry = entry.unwrap();
        assert_eq!(entry.level, "debug");
    }

    #[test]
    fn test_parse_log_line_crit_maps_to_error() {
        let entry = parse_log_line("2024-01-01T00:00:00 kern.crit: critical", "", "");
        assert!(entry.is_some());
        assert_eq!(entry.unwrap().level, "error");
    }

    #[test]
    fn test_parse_log_line_warning_maps_to_warn() {
        let entry = parse_log_line("2024-01-01T00:00:00 kern.warning: warning msg", "", "");
        assert!(entry.is_some());
        assert_eq!(entry.unwrap().level, "warn");
    }

    #[test]
    fn test_parse_log_line_notice_maps_to_info() {
        let entry = parse_log_line("2024-01-01T00:00:00 kern.notice: notice msg", "", "");
        assert!(entry.is_some());
        assert_eq!(entry.unwrap().level, "info");
    }

    #[test]
    fn test_parse_log_line_filter_by_level() {
        // Should match
        let entry = parse_log_line("2024-01-01T00:00:00 kern.err: bad stuff", "error", "");
        assert!(entry.is_some());

        // Should NOT match (line is info level, filter is error)
        let entry = parse_log_line("2024-01-01T00:00:00 kern.info: normal stuff", "error", "");
        assert!(entry.is_none());
    }

    #[test]
    fn test_parse_log_line_filter_by_component() {
        // Component is always "kernel" for dmesg, so "kernel" should match
        let entry = parse_log_line("2024-01-01T00:00:00 kern.info: message", "", "kernel");
        assert!(entry.is_some());

        // Filtering for a different component should return None
        let entry = parse_log_line("2024-01-01T00:00:00 kern.info: message", "", "kubelet");
        assert!(entry.is_none());
    }

    #[test]
    fn test_parse_log_line_no_facility() {
        // Line without facility.level prefix
        let entry = parse_log_line("2024-01-01T00:00:00 plain message without colon", "", "");
        assert!(entry.is_some());
        let entry = entry.unwrap();
        assert_eq!(entry.level, "info");
        assert_eq!(entry.message, "plain message without colon");
    }

    #[test]
    fn test_parse_log_line_empty() {
        // Single word with no spaces
        let entry = parse_log_line("singleword", "", "");
        assert!(entry.is_some());
        let entry = entry.unwrap();
        assert_eq!(entry.level, "info");
        assert_eq!(entry.message, "singleword");
    }

    #[test]
    fn test_parse_log_line_preserves_timestamp() {
        let entry = parse_log_line("2024-06-15T12:30:45 kern.info: msg", "", "");
        assert!(entry.is_some());
        assert_eq!(entry.unwrap().timestamp, "2024-06-15T12:30:45");
    }
}
