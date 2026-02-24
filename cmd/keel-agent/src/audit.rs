//! Audit logging for all gRPC API operations.
//!
//! Provides a Tower layer that intercepts every gRPC request and writes
//! structured JSON audit entries to a persistent log file. Each entry
//! captures the method name, timestamp, response status, and duration.

use chrono::Utc;
use serde::Serialize;
use std::future::Future;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::sync::Mutex;
use tower::{Layer, Service};
use tracing::{info, warn};

/// A single structured audit log entry, serialized as one JSON line.
#[derive(Debug, Clone, Serialize)]
pub struct AuditEntry {
    /// ISO-8601 timestamp of the request
    pub timestamp: String,
    /// gRPC method path (e.g. `/keel.v1.NodeService/GetStatus`)
    pub method: String,
    /// gRPC status code name (e.g. `OK`, `INTERNAL`)
    pub status: String,
    /// Request duration in milliseconds
    pub duration_ms: u64,
}

/// Persistent audit log writer backed by a JSON-lines file.
#[derive(Clone)]
pub struct AuditLog {
    path: PathBuf,
    writer: Arc<Mutex<Option<std::fs::File>>>,
}

impl AuditLog {
    /// Creates a new `AuditLog` targeting the given file path.
    ///
    /// The parent directory is created if it does not exist. If the file
    /// cannot be opened, audit entries are still emitted via `tracing` but
    /// will not be persisted to disk.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let file = Self::open_file(&path);
        Self {
            path,
            writer: Arc::new(Mutex::new(file)),
        }
    }

    /// Attempt to open (or create) the audit log file for appending.
    fn open_file(path: &Path) -> Option<std::fs::File> {
        if let Some(parent) = path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                warn!(error = %e, path = %parent.display(), "Failed to create audit log directory");
                return None;
            }
        }
        match std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            Ok(f) => Some(f),
            Err(e) => {
                warn!(error = %e, path = %path.display(), "Failed to open audit log file");
                None
            }
        }
    }

    /// Write an audit entry to the log file and emit a tracing event.
    pub async fn record(&self, entry: &AuditEntry) {
        info!(
            method = %entry.method,
            status = %entry.status,
            duration_ms = entry.duration_ms,
            "audit"
        );

        if let Ok(line) = serde_json::to_string(entry) {
            let mut guard = self.writer.lock().await;
            if let Some(ref mut file) = *guard {
                let write_result = writeln!(file, "{line}");
                match write_result {
                    Ok(()) => {
                        // Flush to ensure audit entries are durably written
                        if let Err(e) = file.flush() {
                            warn!(error = %e, path = %self.path.display(), "Failed to flush audit entry");
                        }
                    }
                    Err(e) => {
                        warn!(error = %e, path = %self.path.display(), "Failed to write audit entry");
                        // Try to reopen the file on next write
                        *guard = Self::open_file(&self.path);
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tower Layer / Service
// ---------------------------------------------------------------------------

/// Tower [`Layer`] that wraps services with [`AuditService`].
#[derive(Clone)]
pub struct AuditLayer {
    audit_log: AuditLog,
}

impl AuditLayer {
    /// Creates a new audit layer backed by the given [`AuditLog`].
    pub fn new(audit_log: AuditLog) -> Self {
        Self { audit_log }
    }
}

impl<S> Layer<S> for AuditLayer {
    type Service = AuditService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        AuditService {
            inner,
            audit_log: self.audit_log.clone(),
        }
    }
}

/// Tower [`Service`] that records an audit entry for every request.
#[derive(Clone)]
pub struct AuditService<S> {
    inner: S,
    audit_log: AuditLog,
}

impl<S, ReqBody, ResBody> Service<http::Request<ReqBody>> for AuditService<S>
where
    S: Service<http::Request<ReqBody>, Response = http::Response<ResBody>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    S::Error: std::fmt::Display + Send + 'static,
    ReqBody: Send + 'static,
    ResBody: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: http::Request<ReqBody>) -> Self::Future {
        let method = req.uri().path().to_string();
        let start = std::time::Instant::now();
        let audit_log = self.audit_log.clone();

        // Clone the service that was polled ready, then swap it back into `self`
        // so that `self.inner` is the un-polled clone. This ensures we always
        // call a service instance that has been through `poll_ready`, which is
        // required by the Tower Service contract.
        let mut inner = self.inner.clone();
        std::mem::swap(&mut self.inner, &mut inner);

        Box::pin(async move {
            let result = inner.call(req).await;
            let duration_ms = start.elapsed().as_millis() as u64;

            let status = match &result {
                Ok(resp) => grpc_status_from_response(resp),
                Err(e) => format!("ERROR: {e}"),
            };

            let entry = AuditEntry {
                timestamp: Utc::now().to_rfc3339(),
                method,
                status,
                duration_ms,
            };

            audit_log.record(&entry).await;

            result
        })
    }
}

/// Extract the gRPC status from the `grpc-status` header or HTTP status.
fn grpc_status_from_response<B>(resp: &http::Response<B>) -> String {
    // gRPC status may be in headers (for trailers-only responses)
    if let Some(val) = resp.headers().get("grpc-status") {
        if let Ok(s) = val.to_str() {
            return grpc_code_name(s);
        }
    }
    // Fall back to HTTP status
    format!("HTTP {}", resp.status().as_u16())
}

/// Map a numeric gRPC status code to its canonical name.
fn grpc_code_name(code: &str) -> String {
    match code {
        "0" => "OK".to_string(),
        "1" => "CANCELLED".to_string(),
        "2" => "UNKNOWN".to_string(),
        "3" => "INVALID_ARGUMENT".to_string(),
        "4" => "DEADLINE_EXCEEDED".to_string(),
        "5" => "NOT_FOUND".to_string(),
        "6" => "ALREADY_EXISTS".to_string(),
        "7" => "PERMISSION_DENIED".to_string(),
        "8" => "RESOURCE_EXHAUSTED".to_string(),
        "9" => "FAILED_PRECONDITION".to_string(),
        "10" => "ABORTED".to_string(),
        "11" => "OUT_OF_RANGE".to_string(),
        "12" => "UNIMPLEMENTED".to_string(),
        "13" => "INTERNAL".to_string(),
        "14" => "UNAVAILABLE".to_string(),
        "15" => "DATA_LOSS".to_string(),
        "16" => "UNAUTHENTICATED".to_string(),
        other => format!("CODE_{other}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use tempfile::TempDir;

    #[test]
    fn test_grpc_code_name() {
        assert_eq!(grpc_code_name("0"), "OK");
        assert_eq!(grpc_code_name("13"), "INTERNAL");
        assert_eq!(grpc_code_name("16"), "UNAUTHENTICATED");
        assert_eq!(grpc_code_name("99"), "CODE_99");
    }

    #[tokio::test]
    async fn test_audit_log_record_writes_json_line() {
        let dir = TempDir::new().unwrap();
        let log_path = dir.path().join("audit.log");

        let audit_log = AuditLog::new(&log_path);

        let entry = AuditEntry {
            timestamp: "2025-01-01T00:00:00Z".to_string(),
            method: "/keel.v1.NodeService/GetStatus".to_string(),
            status: "OK".to_string(),
            duration_ms: 42,
        };

        audit_log.record(&entry).await;

        let mut contents = String::new();
        std::fs::File::open(&log_path)
            .unwrap()
            .read_to_string(&mut contents)
            .unwrap();

        let parsed: serde_json::Value = serde_json::from_str(contents.trim()).unwrap();
        assert_eq!(parsed["method"], "/keel.v1.NodeService/GetStatus");
        assert_eq!(parsed["status"], "OK");
        assert_eq!(parsed["duration_ms"], 42);
    }

    #[tokio::test]
    async fn test_audit_log_creates_parent_directory() {
        let dir = TempDir::new().unwrap();
        let log_path = dir.path().join("nested").join("deep").join("audit.log");

        let audit_log = AuditLog::new(&log_path);

        let entry = AuditEntry {
            timestamp: "2025-01-01T00:00:00Z".to_string(),
            method: "/test".to_string(),
            status: "OK".to_string(),
            duration_ms: 1,
        };

        audit_log.record(&entry).await;
        assert!(log_path.exists());
    }

    #[tokio::test]
    async fn test_audit_log_appends_multiple_entries() {
        let dir = TempDir::new().unwrap();
        let log_path = dir.path().join("audit.log");

        let audit_log = AuditLog::new(&log_path);

        for i in 0..3 {
            let entry = AuditEntry {
                timestamp: format!("2025-01-01T00:00:0{i}Z"),
                method: format!("/method/{i}"),
                status: "OK".to_string(),
                duration_ms: i,
            };
            audit_log.record(&entry).await;
        }

        let contents = std::fs::read_to_string(&log_path).unwrap();
        let lines: Vec<&str> = contents.trim().lines().collect();
        assert_eq!(lines.len(), 3);

        for (i, line) in lines.iter().enumerate() {
            let parsed: serde_json::Value = serde_json::from_str(line).unwrap();
            assert_eq!(parsed["method"], format!("/method/{i}"));
        }
    }

    #[test]
    fn test_audit_entry_serialization() {
        let entry = AuditEntry {
            timestamp: "2025-06-01T12:00:00Z".to_string(),
            method: "/keel.v1.NodeService/Reboot".to_string(),
            status: "OK".to_string(),
            duration_ms: 5,
        };

        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("\"method\":\"/keel.v1.NodeService/Reboot\""));
        assert!(json.contains("\"duration_ms\":5"));
    }

    #[test]
    fn test_grpc_status_from_response_with_header() {
        let resp = http::Response::builder()
            .header("grpc-status", "0")
            .body(())
            .unwrap();
        assert_eq!(grpc_status_from_response(&resp), "OK");
    }

    #[test]
    fn test_grpc_status_from_response_fallback_http() {
        let resp = http::Response::builder().status(200).body(()).unwrap();
        assert_eq!(grpc_status_from_response(&resp), "HTTP 200");
    }
}
