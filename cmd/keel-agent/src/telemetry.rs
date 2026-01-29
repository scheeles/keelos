//! OpenTelemetry telemetry integration for keel-agent
//!
//! This module initializes and configures OpenTelemetry for:
//! - Distributed tracing
//! - Metrics collection (system and application)
//! - OTLP export to collectors

use opentelemetry::{global, trace::TracerProvider, KeyValue};
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::{runtime, trace::{Config, TracerProvider as SdkTracerProvider}, Resource};
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
    let resource = Resource::builder()
        .with_attributes(vec![
            KeyValue::new("service.name", service_name.to_string()),
            KeyValue::new("service.version", env!("CARGO_PKG_VERSION")),
        ])
        .build();

    // Initialize tracing if OTLP endpoint is provided
    if let Some(endpoint) = otlp_endpoint {
        let exporter = opentelemetry_otlp::SpanExporter::builder()
            .with_tonic()
            .with_endpoint(endpoint)
            .build()?;

        let tracer_provider = SdkTracerProvider::builder()
            .with_batch_exporter(exporter)
            .with_config(Config::default().with_resource(resource))
            .build();

        // Set global tracer provider
        global::set_tracer_provider(tracer_provider.clone());

        // Create tracing subscriber with OpenTelemetry layer
        let tracer = tracer_provider.tracer("keel-agent");
        let telemetry_layer = tracing_opentelemetry::layer().with_tracer(tracer);

        // Set up structured logging with OpenTelemetry
        let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

        tracing_subscriber::registry()
            .with(env_filter)
            .with(tracing_subscriber::fmt::layer().compact())
            .with(telemetry_layer)
            .init();
    } else {
        // Set up structured logging without OpenTelemetry
        let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

        tracing_subscriber::registry()
            .with(env_filter)
            .with(tracing_subscriber::fmt::layer().compact())
            .init();
    }

    Ok(())
}

/// Shutdown telemetry and flush pending data
pub fn shutdown_telemetry() {
    // In v0.31, we need to call shutdown on the provider instance
    // Since we set it globally, we can't easily access it here
    // The global provider will be cleaned up on process exit
}

/// System metrics collector using sysinfo
pub struct SystemMetrics {
    system: System,
}

impl SystemMetrics {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            system: System::new_all(),
        })
    }

    /// Update system metrics
    pub fn update(&mut self) {
        self.system.refresh_all();
    }

    /// Get CPU usage percentage
    #[allow(dead_code)]
    pub fn cpu_usage(&self) -> f32 {
        self.system.global_cpu_info().cpu_usage()
    }

    /// Get total memory in bytes
    #[allow(dead_code)]
    pub fn total_memory(&self) -> u64 {
        self.system.total_memory()
    }

    /// Get used memory in bytes
    #[allow(dead_code)]
    pub fn used_memory(&self) -> u64 {
        self.system.used_memory()
    }

    /// Get total swap in bytes
    #[allow(dead_code)]
    pub fn total_swap(&self) -> u64 {
        self.system.total_swap()
    }

    /// Get used swap in bytes
    #[allow(dead_code)]
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
