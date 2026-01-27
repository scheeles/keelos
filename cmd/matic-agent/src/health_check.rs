//! Health check framework for MaticOS
//!
//! Validates system health after updates with support for:
//! - Pluggable health check implementations
//! - Configurable timeout and retry logic
//! - Multiple check types (boot, service, network, API)
//! - Detailed result tracking

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

/// Result of a health check
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum HealthCheckResult {
    Pass,
    Fail(String),
    Unknown(String),
}

impl HealthCheckResult {
    #[allow(dead_code)]
    pub fn is_passing(&self) -> bool {
        matches!(self, HealthCheckResult::Pass)
    }

    pub fn message(&self) -> String {
        match self {
            HealthCheckResult::Pass => "OK".to_string(),
            HealthCheckResult::Fail(msg) => msg.clone(),
            HealthCheckResult::Unknown(msg) => msg.clone(),
        }
    }
}

/// Overall system health status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
}

impl std::fmt::Display for HealthStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HealthStatus::Healthy => write!(f, "healthy"),
            HealthStatus::Degraded => write!(f, "degraded"),
            HealthStatus::Unhealthy => write!(f, "unhealthy"),
        }
    }
}

/// Individual health check execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckExecution {
    pub name: String,
    pub result: HealthCheckResult,
    pub duration_ms: u64,
}

/// Trait for implementing health checks
#[async_trait]
pub trait HealthCheck: Send + Sync {
    /// Execute the health check
    async fn check(&self) -> HealthCheckResult;

    /// Get the check name
    #[allow(dead_code)]
    fn name(&self) -> String;

    /// Is this a critical check (failure means rollback)
    fn is_critical(&self) -> bool {
        true
    }
}

/// Boot verification check
pub struct BootCheck;

#[async_trait]
impl HealthCheck for BootCheck {
    async fn check(&self) -> HealthCheckResult {
        // Verify system has booted successfully by checking uptime
        match fs::read_to_string("/proc/uptime") {
            Ok(uptime) => {
                let parts: Vec<&str> = uptime.split_whitespace().collect();
                if let Some(uptime_str) = parts.first() {
                    if let Ok(uptime_secs) = uptime_str.parse::<f64>() {
                        if uptime_secs > 10.0 {
                            info!(uptime_secs, "Boot check passed");
                            return HealthCheckResult::Pass;
                        }
                    }
                }
                HealthCheckResult::Fail("System uptime too low".to_string())
            }
            Err(e) => HealthCheckResult::Fail(format!("Cannot read uptime: {}", e)),
        }
    }

    fn name(&self) -> String {
        "boot".to_string()
    }
}

/// Service status check
#[allow(dead_code)]
pub struct ServiceCheck {
    service_name: String,
}

impl ServiceCheck {
    #[allow(dead_code)]
    pub fn new(service_name: impl Into<String>) -> Self {
        Self {
            service_name: service_name.into(),
        }
    }
}

#[async_trait]
impl HealthCheck for ServiceCheck {
    async fn check(&self) -> HealthCheckResult {
        // Check if a service process is running
        let output = Command::new("pgrep")
            .arg("-x")
            .arg(&self.service_name)
            .output();

        match output {
            Ok(result) => {
                if result.status.success() {
                    info!(service = %self.service_name, "Service check passed");
                    HealthCheckResult::Pass
                } else {
                    warn!(service = %self.service_name, "Service not running");
                    HealthCheckResult::Fail(format!("Service {} not running", self.service_name))
                }
            }
            Err(e) => {
                error!(service = %self.service_name, error = %e, "Failed to check service");
                HealthCheckResult::Fail(format!("Cannot check service: {}", e))
            }
        }
    }

    fn name(&self) -> String {
        format!("service:{}", self.service_name)
    }
}

/// Network connectivity check
pub struct NetworkCheck;

#[async_trait]
impl HealthCheck for NetworkCheck {
    async fn check(&self) -> HealthCheckResult {
        // Check if network interfaces are up
        match fs::read_to_string("/proc/net/dev") {
            Ok(content) => {
                let lines: Vec<&str> = content.lines().collect();
                // Skip header lines
                let interface_lines = &lines[2..];

                for line in interface_lines {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if let Some(iface) = parts.first() {
                        let iface_name = iface.trim_end_matches(':');
                        // Skip loopback
                        if iface_name != "lo" && !line.contains("0       0       0") {
                            info!(interface = %iface_name, "Network check passed");
                            return HealthCheckResult::Pass;
                        }
                    }
                }

                warn!("No active network interfaces found");
                HealthCheckResult::Fail("No active network interfaces".to_string())
            }
            Err(e) => HealthCheckResult::Fail(format!("Cannot read network interfaces: {}", e)),
        }
    }

    fn name(&self) -> String {
        "network".to_string()
    }

    fn is_critical(&self) -> bool {
        false // Network failure is degraded, not critical
    }
}

/// API responsiveness check
pub struct ApiCheck {
    port: u16,
}

impl ApiCheck {
    pub fn new(port: u16) -> Self {
        Self { port }
    }
}

#[async_trait]
impl HealthCheck for ApiCheck {
    async fn check(&self) -> HealthCheckResult {
        // Check if the gRPC API port is listening
        let output = Command::new("netstat").arg("-ln").output();

        match output {
            Ok(result) => {
                let stdout = String::from_utf8_lossy(&result.stdout);
                let port_str = format!(":{}", self.port);

                if stdout.contains(&port_str) {
                    info!(port = %self.port, "API check passed");
                    HealthCheckResult::Pass
                } else {
                    warn!(port = %self.port, "API port not listening");
                    HealthCheckResult::Fail(format!("API port {} not listening", self.port))
                }
            }
            Err(e) => {
                error!(error = %e, "Failed to check API port");
                HealthCheckResult::Fail(format!("Cannot check API: {}", e))
            }
        }
    }

    fn name(&self) -> String {
        "api".to_string()
    }
}

/// Health checker configuration
#[derive(Debug, Clone)]
pub struct HealthCheckerConfig {
    #[allow(dead_code)]
    pub timeout_secs: u32,
    #[allow(dead_code)]
    pub retry_interval_secs: u32,
    #[allow(dead_code)]
    pub max_retries: u32,
}

impl Default for HealthCheckerConfig {
    fn default() -> Self {
        Self {
            timeout_secs: 300, // 5 minutes
            retry_interval_secs: 10,
            max_retries: 30,
        }
    }
}

/// Health checker orchestrator
pub struct HealthChecker {
    checks: Arc<RwLock<HashMap<String, Box<dyn HealthCheck>>>>,
    #[allow(dead_code)]
    config: HealthCheckerConfig,
    last_results: Arc<RwLock<Vec<CheckExecution>>>,
}

impl HealthChecker {
    /// Create a new health checker with default checks
    pub fn new(config: HealthCheckerConfig) -> Self {
        let mut checks: HashMap<String, Box<dyn HealthCheck>> = HashMap::new();

        // Register default checks
        checks.insert("boot".to_string(), Box::new(BootCheck));
        checks.insert("network".to_string(), Box::new(NetworkCheck));
        checks.insert("api".to_string(), Box::new(ApiCheck::new(50051)));

        Self {
            checks: Arc::new(RwLock::new(checks)),
            config,
            last_results: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Register a custom health check
    #[allow(dead_code)]
    pub async fn register_check(&self, check: Box<dyn HealthCheck>) {
        let name = check.name();
        let mut checks = self.checks.write().await;
        checks.insert(name.clone(), check);
        info!(check = %name, "Registered health check");
    }

    /// Run all registered health checks
    pub async fn run_all_checks(&self) -> (HealthStatus, Vec<CheckExecution>) {
        let checks = self.checks.read().await;
        let mut executions = Vec::new();
        let mut critical_failures = 0;
        let mut non_critical_failures = 0;

        info!(count = checks.len(), "Running health checks");

        for (name, check) in checks.iter() {
            let start = Instant::now();
            let result = check.check().await;
            let duration_ms = start.elapsed().as_millis() as u64;

            let execution = CheckExecution {
                name: name.clone(),
                result: result.clone(),
                duration_ms,
            };

            executions.push(execution);

            if let HealthCheckResult::Fail(_) = result {
                if check.is_critical() {
                    critical_failures += 1;
                } else {
                    non_critical_failures += 1;
                }
            }
        }

        // Update last results
        let mut last_results = self.last_results.write().await;
        *last_results = executions.clone();

        // Determine overall status
        let status = if critical_failures > 0 {
            HealthStatus::Unhealthy
        } else if non_critical_failures > 0 {
            HealthStatus::Degraded
        } else {
            HealthStatus::Healthy
        };

        info!(
            status = %status.to_string(),
            critical_failures,
            non_critical_failures,
            "Health check completed"
        );

        (status, executions)
    }

    /// Run health checks with retry logic until timeout or success
    #[allow(dead_code)]
    pub async fn run_with_retry(&self) -> HealthStatus {
        let start = Instant::now();
        let timeout = Duration::from_secs(self.config.timeout_secs as u64);
        let retry_interval = Duration::from_secs(self.config.retry_interval_secs as u64);

        let mut attempt = 0;

        loop {
            attempt += 1;
            debug!(attempt, "Running health check attempt");

            let (status, _) = self.run_all_checks().await;

            if status == HealthStatus::Healthy {
                info!(attempt, "Health checks passed");
                return status;
            }

            if start.elapsed() >= timeout {
                warn!(
                    attempt,
                    elapsed_secs = start.elapsed().as_secs(),
                    "Health check timeout reached"
                );
                return status;
            }

            if attempt >= self.config.max_retries {
                warn!(attempt, "Max retry attempts reached");
                return status;
            }

            debug!(
                next_retry_secs = retry_interval.as_secs(),
                "Waiting before retry"
            );
            sleep(retry_interval).await;
        }
    }

    /// Get the last check results
    #[allow(dead_code)]
    pub async fn get_last_results(&self) -> Vec<CheckExecution> {
        let results = self.last_results.read().await;
        results.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_boot_check() {
        let check = BootCheck;
        let result = check.check().await;
        assert!(result.is_passing());
    }

    #[tokio::test]
    async fn test_health_checker() {
        let config = HealthCheckerConfig {
            timeout_secs: 10,
            retry_interval_secs: 1,
            max_retries: 3,
        };

        let checker = HealthChecker::new(config);
        let (status, results) = checker.run_all_checks().await;

        assert!(!results.is_empty());
        // Status depends on environment, just verify it runs
        assert!(matches!(
            status,
            HealthStatus::Healthy | HealthStatus::Degraded | HealthStatus::Unhealthy
        ));
    }
}
