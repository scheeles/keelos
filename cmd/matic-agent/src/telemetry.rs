//! OpenTelemetry telemetry integration for matic-agent
//!
//! This module initializes and configures OpenTelemetry for:
//! - Distributed tracing
//! - Metrics collection (system and application)
//! - OTLP export to collectors

use opentelemetry::{global, KeyValue};
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::{runtime, Resource};
use prometheus::{Encoder, Registry, TextEncoder};
use sysinfo::System;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// Initialize OpenTelemetry with OTLP exporter
///
/// This sets up:
/// - Tracing with OTLP export
/// - Metrics with Prometheus export
/// - Resource attributes (service name, version)
pub fn init_telemetry(
    service_name: &str,
    otlp_endpoint: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Create resource with service information
    let resource = Resource::new(vec![
        KeyValue::new("service.name", service_name.to_string()),
        KeyValue::new("service.version", env!("CARGO_PKG_VERSION")),
    ]);

    // Initialize tracing if OTLP endpoint is provided
    let tracer = if let Some(endpoint) = otlp_endpoint {
        Some(
            opentelemetry_otlp::new_pipeline()
                .tracing()
                .with_exporter(
                    opentelemetry_otlp::new_exporter()
                        .tonic()
                        .with_endpoint(endpoint),
                )
                .with_trace_config(
                    opentelemetry_sdk::trace::config().with_resource(resource.clone()),
                )
                .install_batch(runtime::Tokio)?,
        )
    } else {
        None
    };

    // Create tracing subscriber with OpenTelemetry layer
    let telemetry_layer = tracer
        .as_ref()
        .map(|t| tracing_opentelemetry::layer().with_tracer(t.clone()));

    // Set up structured logging with optional OpenTelemetry
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));

    let subscriber = tracing_subscriber::registry()
        .with(env_filter)
        .with(tracing_subscriber::fmt::layer().compact());

    if let Some(layer) = telemetry_layer {
        subscriber.with(layer).init();
    } else {
        subscriber.init();
    }

    Ok(())
}

/// Shutdown telemetry and flush pending data
pub fn shutdown_telemetry() {
    global::shutdown_tracer_provider();
}

/// System metrics collector
pub struct SystemMetrics {
    system: System,
    registry: Registry,
}

impl SystemMetrics {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let registry = Registry::new();
        Ok(Self {
            system: System::new_all(),
            registry,
        })
    }

    /// Update system metrics
    pub fn update(&mut self) {
        self.system.refresh_all();
    }

    /// Get Prometheus metrics as text
    pub fn export_metrics(&self) -> Result<String, Box<dyn std::error::Error>> {
        let encoder = TextEncoder::new();
        let metric_families = self.registry.gather();
        let mut buffer = Vec::new();
        encoder.encode(&metric_families, &mut buffer)?;
        Ok(String::from_utf8(buffer)?)
    }

    /// Get CPU usage percentage
    pub fn cpu_usage(&self) -> f32 {
        self.system.global_cpu_info().cpu_usage()
    }

    /// Get total memory in bytes
    pub fn total_memory(&self) -> u64 {
        self.system.total_memory()
    }

    /// Get used memory in bytes
    pub fn used_memory(&self) -> u64 {
        self.system.used_memory()
    }

    /// Get total swap in bytes
    pub fn total_swap(&self) -> u64 {
        self.system.total_swap()
    }

    /// Get used swap in bytes
    pub fn used_swap(&self) -> u64 {
        self.system.used_swap()
    }
}

impl Default for SystemMetrics {
    fn default() -> Self {
        Self::new().expect("Failed to create SystemMetrics")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_metrics_creation() {
        let metrics = SystemMetrics::new();
        assert!(metrics.is_ok());
    }

    #[test]
    fn test_system_metrics_update() {
        let mut metrics = SystemMetrics::new().unwrap();
        metrics.update();
        // Just verify it doesn't panic
        let _ = metrics.cpu_usage();
        let _ = metrics.total_memory();
    }
}
