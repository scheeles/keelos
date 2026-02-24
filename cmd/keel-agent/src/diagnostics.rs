//! Diagnostics & debugging module for KeelOS agent.
//!
//! Provides time-limited debug mode, crash dump collection,
//! log streaming, system snapshots, and recovery mode.

use chrono::{DateTime, Duration, Utc};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Maximum allowed debug/recovery session duration (1 hour)
const MAX_SESSION_DURATION_SECS: u32 = 3600;
/// Default session duration if not specified (15 minutes)
const DEFAULT_SESSION_DURATION_SECS: u32 = 900;

/// Tracks the state of a debug session.
#[derive(Clone, Debug)]
pub struct DebugSession {
    /// Unique session identifier
    pub session_id: String,
    /// Reason provided for enabling debug mode
    pub reason: String,
    /// When the session expires
    pub expires_at: DateTime<Utc>,
}

/// Tracks the state of recovery mode.
#[derive(Clone, Debug)]
pub struct RecoverySession {
    /// Reason provided for enabling recovery mode
    pub reason: String,
    /// When recovery mode expires
    pub expires_at: DateTime<Utc>,
}

/// Manages diagnostics state across the agent.
#[derive(Clone)]
pub struct DiagnosticsManager {
    debug_session: Arc<RwLock<Option<DebugSession>>>,
    recovery_session: Arc<RwLock<Option<RecoverySession>>>,
}

impl DiagnosticsManager {
    /// Creates a new `DiagnosticsManager`.
    pub fn new() -> Self {
        Self {
            debug_session: Arc::new(RwLock::new(None)),
            recovery_session: Arc::new(RwLock::new(None)),
        }
    }

    /// Enables a time-limited debug session.
    ///
    /// # Errors
    ///
    /// Returns an error string if a session is already active or the duration is invalid.
    pub async fn enable_debug_mode(
        &self,
        duration_secs: u32,
        reason: &str,
    ) -> Result<DebugSession, String> {
        let mut session = self.debug_session.write().await;

        // Check if already active
        if let Some(existing) = session.as_ref() {
            if existing.expires_at > Utc::now() {
                return Err(format!(
                    "Debug mode already active (session: {}, expires: {})",
                    existing.session_id,
                    existing.expires_at.to_rfc3339()
                ));
            }
        }

        let duration = clamp_duration(duration_secs);
        let expires_at = Utc::now() + Duration::seconds(i64::from(duration));
        let session_id = uuid::Uuid::new_v4().to_string();

        let new_session = DebugSession {
            session_id: session_id.clone(),
            reason: reason.to_string(),
            expires_at,
        };

        info!(
            session_id = %session_id,
            reason = %reason,
            duration_secs = duration,
            expires_at = %expires_at.to_rfc3339(),
            "DEBUG MODE ENABLED (audit)"
        );

        *session = Some(new_session.clone());
        Ok(new_session)
    }

    /// Returns the current debug session status.
    pub async fn get_debug_status(&self) -> Option<DebugSession> {
        let session = self.debug_session.read().await;
        if let Some(s) = session.as_ref() {
            if s.expires_at > Utc::now() {
                return Some(s.clone());
            }
        }
        None
    }

    /// Enables time-limited recovery mode.
    ///
    /// # Errors
    ///
    /// Returns an error string if recovery mode is already active.
    pub async fn enable_recovery_mode(
        &self,
        duration_secs: u32,
        reason: &str,
    ) -> Result<RecoverySession, String> {
        let mut session = self.recovery_session.write().await;

        // Check if already active
        if let Some(existing) = session.as_ref() {
            if existing.expires_at > Utc::now() {
                return Err(format!(
                    "Recovery mode already active (expires: {})",
                    existing.expires_at.to_rfc3339()
                ));
            }
        }

        let duration = clamp_duration(duration_secs);
        let expires_at = Utc::now() + Duration::seconds(i64::from(duration));

        let new_session = RecoverySession {
            reason: reason.to_string(),
            expires_at,
        };

        info!(
            reason = %reason,
            duration_secs = duration,
            expires_at = %expires_at.to_rfc3339(),
            "RECOVERY MODE ENABLED (audit)"
        );

        *session = Some(new_session.clone());
        Ok(new_session)
    }
}

/// Collects a crash dump from the system.
///
/// Gathers kernel (dmesg) and/or userspace process information
/// and writes it to `/var/lib/keel/crash-dumps/`.
///
/// # Errors
///
/// Returns an error string if dump collection fails.
pub fn collect_crash_dump(
    include_kernel: bool,
    include_userspace: bool,
) -> Result<(String, u64), String> {
    collect_crash_dump_to(
        "/var/lib/keel/crash-dumps",
        include_kernel,
        include_userspace,
    )
}

/// Internal implementation of crash dump collection with configurable output directory.
///
/// # Errors
///
/// Returns an error string if dump collection fails.
fn collect_crash_dump_to(
    dump_dir: &str,
    include_kernel: bool,
    include_userspace: bool,
) -> Result<(String, u64), String> {
    std::fs::create_dir_all(dump_dir)
        .map_err(|e| format!("Failed to create crash dump directory: {e}"))?;

    let timestamp = Utc::now().format("%Y%m%d-%H%M%S");
    let dump_path = format!("{dump_dir}/crash-{timestamp}.txt");
    let mut content = String::new();

    content.push_str(&format!(
        "=== KeelOS Crash Dump - {} ===\n\n",
        Utc::now().to_rfc3339()
    ));

    if include_kernel {
        debug!("Collecting kernel crash data (dmesg)");
        content.push_str("--- Kernel Messages (dmesg) ---\n");
        match std::process::Command::new("dmesg")
            .arg("--time-format=iso")
            .output()
        {
            Ok(output) => {
                content.push_str(&String::from_utf8_lossy(&output.stdout));
            }
            Err(e) => {
                content.push_str(&format!("Failed to collect dmesg: {e}\n"));
                warn!(error = %e, "Failed to collect dmesg for crash dump");
            }
        }
        content.push('\n');
    }

    if include_userspace {
        debug!("Collecting userspace process info");
        content.push_str("--- Process List ---\n");
        match std::process::Command::new("ps").arg("aux").output() {
            Ok(output) => {
                content.push_str(&String::from_utf8_lossy(&output.stdout));
            }
            Err(e) => {
                content.push_str(&format!("Failed to collect process list: {e}\n"));
                warn!(error = %e, "Failed to collect ps output for crash dump");
            }
        }
        content.push('\n');

        content.push_str("--- Memory Info ---\n");
        match std::fs::read_to_string("/proc/meminfo") {
            Ok(meminfo) => content.push_str(&meminfo),
            Err(e) => {
                content.push_str(&format!("Failed to read /proc/meminfo: {e}\n"));
            }
        }
        content.push('\n');
    }

    let dump_size = content.len() as u64;
    std::fs::write(&dump_path, &content).map_err(|e| format!("Failed to write crash dump: {e}"))?;

    info!(path = %dump_path, size = dump_size, "Crash dump collected");
    Ok((dump_path, dump_size))
}

/// Creates a system snapshot including config and/or logs.
///
/// # Errors
///
/// Returns an error string if snapshot creation fails.
pub fn create_system_snapshot(
    label: &str,
    include_config: bool,
    include_logs: bool,
) -> Result<(String, String, u64), String> {
    create_system_snapshot_to(
        "/var/lib/keel/snapshots",
        label,
        include_config,
        include_logs,
    )
}

/// Internal implementation of system snapshot creation with configurable output directory.
///
/// # Errors
///
/// Returns an error string if snapshot creation fails.
fn create_system_snapshot_to(
    snapshot_dir: &str,
    label: &str,
    include_config: bool,
    include_logs: bool,
) -> Result<(String, String, u64), String> {
    std::fs::create_dir_all(snapshot_dir)
        .map_err(|e| format!("Failed to create snapshot directory: {e}"))?;

    let snapshot_id = uuid::Uuid::new_v4().to_string();
    let timestamp = Utc::now().format("%Y%m%d-%H%M%S");
    let snapshot_path = format!("{snapshot_dir}/snapshot-{timestamp}.txt");
    let mut content = String::new();

    content.push_str(&format!(
        "=== KeelOS System Snapshot ===\nID: {snapshot_id}\nLabel: {label}\nCreated: {}\n\n",
        Utc::now().to_rfc3339()
    ));

    if include_config {
        debug!("Including system configuration in snapshot");
        content.push_str("--- System Configuration ---\n");

        // Capture hostname
        match std::fs::read_to_string("/etc/hostname") {
            Ok(hostname) => {
                content.push_str(&format!("Hostname: {}\n", hostname.trim()));
            }
            Err(_) => content.push_str("Hostname: unavailable\n"),
        }

        // Capture OS version info
        content.push_str("\nOS Release:\n");
        match std::fs::read_to_string("/etc/os-release") {
            Ok(release) => content.push_str(&release),
            Err(_) => content.push_str("  unavailable\n"),
        }

        // Capture keel config if present
        let config_path = "/etc/keel/node.yaml";
        if std::path::Path::new(config_path).exists() {
            content.push_str("\nKeelOS Node Config:\n");
            match std::fs::read_to_string(config_path) {
                Ok(cfg) => content.push_str(&cfg),
                Err(e) => content.push_str(&format!("  Failed to read: {e}\n")),
            }
        }

        content.push('\n');
    }

    if include_logs {
        debug!("Including logs in snapshot");
        content.push_str("--- Recent Kernel Logs ---\n");
        match std::process::Command::new("dmesg")
            .arg("--time-format=iso")
            .output()
        {
            Ok(output) => {
                let full = String::from_utf8_lossy(&output.stdout);
                // Include only last 200 lines
                let lines: Vec<&str> = full.lines().collect();
                let start = if lines.len() > 200 {
                    lines.len() - 200
                } else {
                    0
                };
                for line in &lines[start..] {
                    content.push_str(line);
                    content.push('\n');
                }
            }
            Err(e) => {
                content.push_str(&format!("Failed to collect dmesg: {e}\n"));
            }
        }
        content.push('\n');
    }

    let size = content.len() as u64;
    std::fs::write(&snapshot_path, &content)
        .map_err(|e| format!("Failed to write snapshot: {e}"))?;

    info!(
        snapshot_id = %snapshot_id,
        path = %snapshot_path,
        label = %label,
        size = size,
        "System snapshot created"
    );

    Ok((snapshot_id, snapshot_path, size))
}

/// Clamp a duration to valid bounds.
fn clamp_duration(duration_secs: u32) -> u32 {
    if duration_secs == 0 {
        DEFAULT_SESSION_DURATION_SECS
    } else if duration_secs > MAX_SESSION_DURATION_SECS {
        MAX_SESSION_DURATION_SECS
    } else {
        duration_secs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clamp_duration_zero() {
        assert_eq!(clamp_duration(0), DEFAULT_SESSION_DURATION_SECS);
    }

    #[test]
    fn test_clamp_duration_within_bounds() {
        assert_eq!(clamp_duration(600), 600);
    }

    #[test]
    fn test_clamp_duration_exceeds_max() {
        assert_eq!(clamp_duration(7200), MAX_SESSION_DURATION_SECS);
    }

    #[test]
    fn test_diagnostics_manager_new() {
        let mgr = DiagnosticsManager::new();
        assert!(mgr.debug_session.try_read().is_ok());
    }

    #[tokio::test]
    async fn test_enable_debug_mode() {
        let mgr = DiagnosticsManager::new();
        let session = mgr.enable_debug_mode(300, "testing").await;
        assert!(session.is_ok());
        let session = session.unwrap_or_else(|e| panic!("unexpected error: {e}"));
        assert!(!session.session_id.is_empty());
        assert_eq!(session.reason, "testing");
        assert!(session.expires_at > Utc::now());
    }

    #[tokio::test]
    async fn test_enable_debug_mode_rejects_duplicate() {
        let mgr = DiagnosticsManager::new();
        let _ = mgr.enable_debug_mode(300, "first").await;
        let result = mgr.enable_debug_mode(300, "second").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_debug_status_active() {
        let mgr = DiagnosticsManager::new();
        let _ = mgr.enable_debug_mode(300, "test").await;
        let status = mgr.get_debug_status().await;
        assert!(status.is_some());
    }

    #[tokio::test]
    async fn test_get_debug_status_inactive() {
        let mgr = DiagnosticsManager::new();
        let status = mgr.get_debug_status().await;
        assert!(status.is_none());
    }

    #[tokio::test]
    async fn test_enable_recovery_mode() {
        let mgr = DiagnosticsManager::new();
        let session = mgr.enable_recovery_mode(300, "emergency").await;
        assert!(session.is_ok());
        let session = session.unwrap_or_else(|e| panic!("unexpected error: {e}"));
        assert_eq!(session.reason, "emergency");
        assert!(session.expires_at > Utc::now());
    }

    #[tokio::test]
    async fn test_enable_recovery_mode_rejects_duplicate() {
        let mgr = DiagnosticsManager::new();
        let _ = mgr.enable_recovery_mode(300, "first").await;
        let result = mgr.enable_recovery_mode(300, "second").await;
        assert!(result.is_err());
    }

    #[test]
    fn test_collect_crash_dump_creates_file() {
        let tmp_dir =
            std::env::temp_dir().join(format!("keel-test-crash-{}", uuid::Uuid::new_v4()));
        let dump_dir = tmp_dir
            .to_str()
            .unwrap_or_else(|| panic!("temp dir path is not valid UTF-8"));

        let result = collect_crash_dump_to(dump_dir, false, false);
        assert!(result.is_ok());

        let (path, size) = result.unwrap_or_else(|e| panic!("unexpected error: {e}"));
        assert!(path.starts_with(dump_dir));
        assert!(path.contains("crash-"));
        assert!(size > 0);

        // Verify file exists and contains the header
        let content =
            std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("failed to read dump: {e}"));
        assert!(content.contains("=== KeelOS Crash Dump"));

        // Cleanup
        let _ = std::fs::remove_dir_all(&tmp_dir);
    }

    #[test]
    fn test_collect_crash_dump_with_kernel() {
        let tmp_dir =
            std::env::temp_dir().join(format!("keel-test-crash-k-{}", uuid::Uuid::new_v4()));
        let dump_dir = tmp_dir
            .to_str()
            .unwrap_or_else(|| panic!("temp dir path is not valid UTF-8"));

        let result = collect_crash_dump_to(dump_dir, true, false);
        assert!(result.is_ok());

        let (path, _) = result.unwrap_or_else(|e| panic!("unexpected error: {e}"));
        let content =
            std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("failed to read dump: {e}"));
        assert!(content.contains("--- Kernel Messages (dmesg) ---"));

        let _ = std::fs::remove_dir_all(&tmp_dir);
    }

    #[test]
    fn test_collect_crash_dump_with_userspace() {
        let tmp_dir =
            std::env::temp_dir().join(format!("keel-test-crash-u-{}", uuid::Uuid::new_v4()));
        let dump_dir = tmp_dir
            .to_str()
            .unwrap_or_else(|| panic!("temp dir path is not valid UTF-8"));

        let result = collect_crash_dump_to(dump_dir, false, true);
        assert!(result.is_ok());

        let (path, _) = result.unwrap_or_else(|e| panic!("unexpected error: {e}"));
        let content =
            std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("failed to read dump: {e}"));
        assert!(content.contains("--- Process List ---"));
        assert!(content.contains("--- Memory Info ---"));

        let _ = std::fs::remove_dir_all(&tmp_dir);
    }

    #[test]
    fn test_create_system_snapshot_creates_file() {
        let tmp_dir = std::env::temp_dir().join(format!("keel-test-snap-{}", uuid::Uuid::new_v4()));
        let snap_dir = tmp_dir
            .to_str()
            .unwrap_or_else(|| panic!("temp dir path is not valid UTF-8"));

        let result = create_system_snapshot_to(snap_dir, "test-label", false, false);
        assert!(result.is_ok());

        let (snapshot_id, path, size) = result.unwrap_or_else(|e| panic!("unexpected error: {e}"));
        assert!(!snapshot_id.is_empty());
        assert!(path.starts_with(snap_dir));
        assert!(path.contains("snapshot-"));
        assert!(size > 0);

        // Verify file exists and contains expected content
        let content = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("failed to read snapshot: {e}"));
        assert!(content.contains("=== KeelOS System Snapshot ==="));
        assert!(content.contains(&snapshot_id));
        assert!(content.contains("test-label"));

        let _ = std::fs::remove_dir_all(&tmp_dir);
    }

    #[test]
    fn test_create_system_snapshot_with_config() {
        let tmp_dir =
            std::env::temp_dir().join(format!("keel-test-snap-c-{}", uuid::Uuid::new_v4()));
        let snap_dir = tmp_dir
            .to_str()
            .unwrap_or_else(|| panic!("temp dir path is not valid UTF-8"));

        let result = create_system_snapshot_to(snap_dir, "config-snap", true, false);
        assert!(result.is_ok());

        let (_, path, _) = result.unwrap_or_else(|e| panic!("unexpected error: {e}"));
        let content = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("failed to read snapshot: {e}"));
        assert!(content.contains("--- System Configuration ---"));
        assert!(content.contains("Hostname:"));

        let _ = std::fs::remove_dir_all(&tmp_dir);
    }

    #[test]
    fn test_create_system_snapshot_with_logs() {
        let tmp_dir =
            std::env::temp_dir().join(format!("keel-test-snap-l-{}", uuid::Uuid::new_v4()));
        let snap_dir = tmp_dir
            .to_str()
            .unwrap_or_else(|| panic!("temp dir path is not valid UTF-8"));

        let result = create_system_snapshot_to(snap_dir, "log-snap", false, true);
        assert!(result.is_ok());

        let (_, path, _) = result.unwrap_or_else(|e| panic!("unexpected error: {e}"));
        let content = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("failed to read snapshot: {e}"));
        assert!(content.contains("--- Recent Kernel Logs ---"));

        let _ = std::fs::remove_dir_all(&tmp_dir);
    }
}
