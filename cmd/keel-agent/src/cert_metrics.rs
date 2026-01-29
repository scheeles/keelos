//! Certificate metrics for OpenTelemetry
//!
//! Provides metrics for monitoring certificate lifecycle:
//! - Certificate expiry time
//! - Renewal success/failure counts
//! - Certificate age

use keel_crypto::parse_cert_expiry;
use opentelemetry::{global, metrics::Counter, KeyValue};
use std::sync::{Arc, Mutex};
use tracing::{error, info};

/// Certificate state for metrics
#[derive(Debug, Clone)]
struct CertState {
    pub expiry_timestamp: i64,
    pub days_remaining: i64,
}

/// Certificate metrics collector
pub struct CertificateMetrics {
    /// Counter for successful certificate renewals
    renewals_success_counter: Counter<u64>,

    /// Counter for failed certificate renewals
    renewals_error_counter: Counter<u64>,

    /// Current certificate state
    cert_state: Arc<Mutex<Option<CertState>>>,
}

impl CertificateMetrics {
    /// Create new certificate metrics collector
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let meter = global::meter("keel_agent");

        let cert_state: Arc<Mutex<Option<CertState>>> = Arc::new(Mutex::new(None));

        // Register observable gauges
        let state_for_expiry = cert_state.clone();
        let _expiry_gauge = meter
            .i64_observable_gauge("keel.certificate.expiry_timestamp")
            .with_description("Certificate expiry time as Unix timestamp")
            .with_callback(move |observer| {
                if let Ok(state) = state_for_expiry.lock() {
                    if let Some(s) = &*state {
                        observer.observe(s.expiry_timestamp, &[]);
                    }
                }
            })
            .init();

        let state_for_days = cert_state.clone();
        let _days_gauge = meter
            .i64_observable_gauge("keel.certificate.days_remaining")
            .with_description("Days until certificate expires")
            .with_callback(move |observer| {
                if let Ok(state) = state_for_days.lock() {
                    if let Some(s) = &*state {
                        observer.observe(s.days_remaining, &[]);
                    }
                }
            })
            .init();

        Ok(Self {
            renewals_success_counter: meter
                .u64_counter("keel.certificate.renewals.success")
                .with_description("Count of successful certificate renewals")
                .init(),

            renewals_error_counter: meter
                .u64_counter("keel.certificate.renewals.errors")
                .with_description("Count of failed certificate renewal attempts")
                .init(),

            cert_state,
        })
    }
    /// Update certificate expiry metrics from certificate file
    pub fn update_cert_expiry(&self, cert_path: &str) {
        match std::fs::read_to_string(cert_path) {
            Ok(cert_pem) => match parse_cert_expiry(&cert_pem) {
                Ok(expiry) => {
                    let expiry_timestamp = expiry.timestamp();
                    let now = chrono::Utc::now().timestamp();
                    let days_remaining = (expiry_timestamp - now) / 86400; // seconds to days

                    // Update state
                    if let Ok(mut state) = self.cert_state.lock() {
                        *state = Some(CertState {
                            expiry_timestamp,
                            days_remaining,
                        });
                    }

                    info!(
                        cert_path = cert_path,
                        expiry = %expiry,
                        days_remaining = days_remaining,
                        "Certificate expiry metrics updated"
                    );
                }
                Err(e) => {
                    error!(cert_path = cert_path, error = %e, "Failed to parse certificate expiry");
                }
            },
            Err(e) => {
                error!(cert_path = cert_path, error = %e, "Failed to read certificate file");
            }
        }
    }

    /// Record successful certificate renewal
    pub fn record_renewal_success(&self, cert_type: &str) {
        self.renewals_success_counter
            .add(1, &[KeyValue::new("cert_type", cert_type.to_string())]);
        info!(
            cert_type = cert_type,
            "Certificate renewal success recorded"
        );
    }

    /// Record failed certificate renewal
    pub fn record_renewal_error(&self, cert_type: &str, error: &str) {
        self.renewals_error_counter.add(
            1,
            &[
                KeyValue::new("cert_type", cert_type.to_string()),
                KeyValue::new("error", error.to_string()),
            ],
        );
        error!(
            cert_type = cert_type,
            error = error,
            "Certificate renewal error recorded"
        );
    }
}

impl Default for CertificateMetrics {
    fn default() -> Self {
        Self::new().expect("Failed to create CertificateMetrics")
    }
}

/// Global certificate metrics instance
static CERT_METRICS: once_cell::sync::Lazy<Arc<CertificateMetrics>> =
    once_cell::sync::Lazy::new(|| {
        Arc::new(CertificateMetrics::new().expect("Failed to initialize certificate metrics"))
    });

/// Get global certificate metrics instance
pub fn cert_metrics() -> Arc<CertificateMetrics> {
    CERT_METRICS.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_certificate_metrics_creation() {
        let metrics = CertificateMetrics::new();
        assert!(metrics.is_ok());
    }
}
