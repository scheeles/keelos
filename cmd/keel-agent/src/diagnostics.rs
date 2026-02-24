use async_stream::try_stream;
use chrono::{DateTime, Duration, Utc};
use keel_api::node::{CrashDumpData, LogEntry, SnapshotData};
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::RwLock;
use tokio_stream::Stream;
use tracing::{info, warn};

pub struct DebugMode {
    pub enabled: bool,
    pub expires_at: Option<DateTime<Utc>>,
    pub reason: String,
}

pub struct DiagnosticsManager {
    debug_mode: Arc<RwLock<DebugMode>>,
}

impl DiagnosticsManager {
    pub fn new() -> Self {
        Self {
            debug_mode: Arc::new(RwLock::new(DebugMode {
                enabled: false,
                expires_at: None,
                reason: String::new(),
            })),
        }
    }

    pub async fn enable_debug_mode(
        &self,
        duration_mins: u32,
        reason: String,
    ) -> (bool, String, Option<DateTime<Utc>>) {
        let mut mode = self.debug_mode.write().await;
        let expires_at = Utc::now() + Duration::minutes(duration_mins as i64);

        mode.enabled = true;
        mode.expires_at = Some(expires_at);
        mode.reason = reason.clone();

        info!(reason = %reason, expires_at = %expires_at.to_rfc3339(), "Debug mode enabled");

        (
            true,
            format!("Debug mode enabled for {} minutes", duration_mins),
            Some(expires_at),
        )
    }

    pub async fn disable_debug_mode(&self, reason: String) -> bool {
        let mut mode = self.debug_mode.write().await;
        mode.enabled = false;
        mode.expires_at = None;

        info!(reason = %reason, "Debug mode disabled");
        true
    }

    #[allow(dead_code)]
    pub async fn is_debug_mode_enabled(&self) -> bool {
        let mut mode = self.debug_mode.write().await;
        if mode.enabled {
            if let Some(expires_at) = mode.expires_at {
                if Utc::now() > expires_at {
                    mode.enabled = false;
                    mode.expires_at = None;
                    warn!("Debug mode expired");
                    return false;
                }
            }
            return true;
        }
        false
    }

    pub fn stream_logs(
        &self,
        filter: String,
        follow: bool,
        tail: u32,
    ) -> impl Stream<Item = Result<LogEntry, tonic::Status>> {
        try_stream! {
            let _: () = (); // Help inference
            let mut args = vec!["-o", "short-iso"];
            if follow {
                args.push("-f");
            }
            let tail_str = tail.to_string();
            if tail > 0 {
                args.push("-n");
                args.push(&tail_str);
            }
            if !filter.is_empty() {
                args.push("-u");
                args.push(&filter);
            }

            let mut child = Command::new("journalctl")
                .args(&args)
                .stdout(Stdio::piped())
                .kill_on_drop(true)
                .spawn()
                .map_err(|e| tonic::Status::internal(format!("Failed to spawn journalctl: {}", e)))?;

            let stdout = child.stdout.take().unwrap();
            let mut reader = BufReader::new(stdout).lines();

            while let Some(line) = reader.next_line().await.map_err(|e| tonic::Status::internal(format!("Read error: {}", e)))? {
                let parts: Vec<&str> = line.splitn(4, ' ').collect();
                if parts.len() >= 4 {
                    yield LogEntry {
                        timestamp: parts[0].to_string(),
                        level: "info".to_string(),
                        source: parts[2].trim_end_matches(':').to_string(),
                        message: parts[3].to_string(),
                    };
                } else {
                    yield LogEntry {
                        timestamp: Utc::now().to_rfc3339(),
                        level: "info".to_string(),
                        source: "journal".to_string(),
                        message: line,
                    };
                }
            }
        }
    }

    pub fn collect_crash_dumps(
        &self,
        since: String,
    ) -> impl Stream<Item = Result<CrashDumpData, tonic::Status>> {
        let since_dt = if !since.is_empty() {
            DateTime::parse_from_rfc3339(&since)
                .ok()
                .map(|dt| dt.with_timezone(&Utc))
        } else {
            None
        };

        try_stream! {
            let _: () = (); // Help inference
            let coredump_dir = "/var/lib/systemd/coredump";
            if std::path::Path::new(coredump_dir).exists() {
                let mut entries = tokio::fs::read_dir(coredump_dir).await
                    .map_err(|e| tonic::Status::internal(format!("Failed to read coredump dir: {}", e)))?;

                while let Some(entry) = entries.next_entry().await.map_err(|e| tonic::Status::internal(e.to_string()))? {
                    let path = entry.path();
                    if path.is_file() {
                        let metadata = entry.metadata().await.map_err(|e| tonic::Status::internal(e.to_string()))?;
                        let modified: DateTime<Utc> = metadata.modified().unwrap_or(std::time::SystemTime::now()).into();

                        if let Some(since) = since_dt {
                            if modified < since {
                                continue;
                            }
                        }

                        let mut file = tokio::fs::File::open(&path).await.map_err(|e| tonic::Status::internal(e.to_string()))?;
                        let total_size = metadata.len();
                        let mut buffer = vec![0u8; 64 * 1024]; // 64KB chunks

                        while let Ok(n) = file.read(&mut buffer).await {
                            if n == 0 { break; }
                            yield CrashDumpData {
                                filename: path.file_name().unwrap().to_string_lossy().to_string(),
                                total_size,
                                data: buffer[..n].to_vec(),
                            };
                        }
                    }
                }
            }
        }
    }

    pub fn create_system_snapshot(
        &self,
        include_logs: bool,
        include_config: bool,
    ) -> impl Stream<Item = Result<SnapshotData, tonic::Status>> {
        try_stream! {
            let _: () = (); // Help inference
            let snapshot_id = uuid::Uuid::new_v4();
            let snapshot_path = format!("/tmp/keelos-snapshot-{}.tar.gz", snapshot_id);
            let mut args = vec!["-czf", &snapshot_path];

            let mut targets = vec![];
            if include_config {
                if std::path::Path::new("/etc/keel").exists() { targets.push("/etc/keel"); }
                if std::path::Path::new("/var/lib/keel").exists() { targets.push("/var/lib/keel"); }
            }
            if include_logs && std::path::Path::new("/var/log/journal").exists() {
                targets.push("/var/log/journal");
            }

            if targets.is_empty() {
                return;
            }

            args.extend(targets);

            let status: std::process::ExitStatus = Command::new("tar")
                .args(&args)
                .status()
                .await
                .map_err(|e| tonic::Status::internal(format!("Failed to run tar: {}", e)))?;

            if !status.success() {
                Err(tonic::Status::internal("Tar failed"))?;
            }

            let mut file = tokio::fs::File::open(&snapshot_path).await.map_err(|e| tonic::Status::internal(e.to_string()))?;
            let total_size = file.metadata().await.map(|m| m.len()).unwrap_or(0);
            let mut buffer = vec![0u8; 128 * 1024]; // 128KB chunks

            while let Ok(n) = file.read(&mut buffer).await {
                if n == 0 { break; }
                yield SnapshotData {
                    data: buffer[..n].to_vec(),
                    total_size,
                };
            }

            let _ = tokio::fs::remove_file(&snapshot_path).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_stream::StreamExt;

    #[tokio::test]
    async fn test_debug_mode_toggle() {
        let manager = DiagnosticsManager::new();

        // Initial state
        assert!(!manager.is_debug_mode_enabled().await);

        // Enable
        let (success, _, expires_at) = manager.enable_debug_mode(10, "test".to_string()).await;
        assert!(success);
        assert!(expires_at.is_some());
        assert!(manager.is_debug_mode_enabled().await);

        // Disable
        let success = manager.disable_debug_mode("test-disable".to_string()).await;
        assert!(success);
        assert!(!manager.is_debug_mode_enabled().await);
    }

    #[tokio::test]
    async fn test_debug_mode_expiry() {
        let manager = DiagnosticsManager::new();

        // Enable with 0 minutes (expires immediately or next check)
        // Actually our logic is Utc::now() + duration.
        // Let's use a very short duration if we could, but it's u32 minutes.
        // We can manually manipulate the state if we want to test expiry perfectly,
        // but let's just test that it works with positive duration.
        manager.enable_debug_mode(60, "test".to_string()).await;
        assert!(manager.is_debug_mode_enabled().await);

        // Manually expire it by reaching into the lock
        {
            let mut mode = manager.debug_mode.write().await;
            mode.expires_at = Some(Utc::now() - Duration::minutes(1));
        }

        assert!(!manager.is_debug_mode_enabled().await);
    }

    #[tokio::test]
    async fn test_stream_logs_mock() {
        let manager = DiagnosticsManager::new();
        // This will try to call journalctl, which might fail in test env if not present
        // but we can at least check it returns a stream.
        let stream = manager.stream_logs(String::new(), false, 1);
        // We don't necessarily expect output in a restricted test env
        let _ = stream.collect::<Vec<_>>().await;
    }
}
