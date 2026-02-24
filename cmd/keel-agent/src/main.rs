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

use keel_api::node::node_service_server::NodeServiceServer;
use std::sync::Arc;
use tokio::sync::RwLock;
use tonic::transport::Server;
use tracing::{error, info, warn};

use keel_agent::disk;
use keel_agent::health;
use keel_agent::health_check;
use keel_agent::hooks::execute_hook;
use keel_agent::mtls::TlsManager;
use keel_agent::telemetry;
use keel_agent::update_scheduler;
use keel_agent::{
    HealthChecker, HealthCheckerConfig, HelperNodeService, ScheduleStatus, UpdateScheduler,
};

/// Initialize operational certificates if running in Kubernetes
/// Returns (cert_path, key_path) if successful, None if not in K8s or on error
async fn init_k8s_certificates() -> Option<(String, String)> {
    use keel_agent::k8s_csr::K8sCsrManager;

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

    // Initialize K8s operational certificates if running in cluster
    if let Some((cert_path, key_path)) = init_k8s_certificates().await {
        info!("K8s operational certificates initialized:");
        info!("  Cert: {}", cert_path);
        info!("  Key: {}", key_path);
    }

    // Start certificate auto-renewal daemon if operational cert exists
    let operational_cert_path = "/var/lib/keel/crypto/operational.pem";
    if std::path::Path::new(operational_cert_path).exists() {
        use keel_agent::cert_renewal::{CertRenewalConfig, CertRenewalManager};

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
            // Enforce maintenance window
            if !UpdateScheduler::is_within_maintenance_window(&schedule) {
                warn!(
                    schedule_id = %schedule.id,
                    "Skipping scheduled update: outside maintenance window"
                );
                let _ = scheduler
                    .update_status(
                        &schedule.id,
                        ScheduleStatus::Failed,
                        Some("Maintenance window expired".to_string()),
                    )
                    .await;
                continue;
            }

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

    // Record active partition before switching for rollback support
    if let Err(e) = disk::record_active_partition_for_rollback() {
        warn!(error = %e, "Failed to record active partition for rollback");
    }

    // Switch boot partition
    disk::switch_boot_partition(inactive.index).map_err(|e| e.to_string())?;

    Ok(())
}

/// Default grace period (in seconds) before running post-boot health checks
const DEFAULT_HEALTH_CHECK_GRACE_SECS: u64 = 60;

/// Rollback supervisor checks health after boot and triggers rollback if critical
async fn start_rollback_supervisor(health: Arc<HealthChecker>, scheduler: Arc<UpdateScheduler>) {
    use tokio::time::{sleep, Duration};

    // Use health check timeout from the latest schedule if available
    let grace_secs = scheduler
        .get_latest_active_schedule()
        .await
        .and_then(|s| s.health_check_timeout_secs)
        .map_or(DEFAULT_HEALTH_CHECK_GRACE_SECS, u64::from);

    info!(
        grace_secs = grace_secs,
        "Rollback supervisor started - waiting for system stability"
    );
    sleep(Duration::from_secs(grace_secs)).await;

    // Run critical check
    let (status, _) = health.run_all_checks().await;

    if status == health_check::HealthStatus::Unhealthy {
        error!(status = %status, "Critical health failure detected!");

        // Check if the latest update had auto-rollback enabled
        let auto_rollback_enabled = scheduler
            .get_latest_active_schedule()
            .await
            .is_some_and(|s| s.enable_auto_rollback);

        if !auto_rollback_enabled {
            warn!("Auto-rollback is not enabled for the latest update schedule; skipping automatic rollback");
            return;
        }

        warn!("Initiating AUTOMATIC ROLLBACK due to critical health failure");

        // Persist rollback state before rebooting
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
