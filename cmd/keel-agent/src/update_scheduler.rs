//! Update scheduler for KeelOS
//!
//! Handles scheduling updates for future execution with:
//! - Persistent schedule storage
//! - Maintenance window support
//! - Auto-rollback configuration
//! - Update hooks (pre/post)

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};
use uuid::Uuid;

/// Update schedule entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateSchedule {
    pub id: String,
    pub source_url: String,
    pub expected_sha256: Option<String>,
    pub scheduled_at: Option<DateTime<Utc>>,
    pub maintenance_window_secs: Option<u32>,
    pub enable_auto_rollback: bool,
    pub pre_update_hook: Option<String>,
    pub post_update_hook: Option<String>,
    pub health_check_timeout_secs: Option<u32>,
    // Delta support
    pub is_delta: bool,
    pub fallback_to_full: bool,
    pub full_image_url: Option<String>,
    pub rollback_triggered: bool,
    pub rollback_reason: Option<String>,
    pub status: ScheduleStatus,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ScheduleStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
    RolledBack,
}

impl std::fmt::Display for ScheduleStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScheduleStatus::Pending => write!(f, "pending"),
            ScheduleStatus::Running => write!(f, "running"),
            ScheduleStatus::Completed => write!(f, "completed"),
            ScheduleStatus::Failed => write!(f, "failed"),
            ScheduleStatus::Cancelled => write!(f, "cancelled"),
            ScheduleStatus::RolledBack => write!(f, "rolled_back"),
        }
    }
}

/// Update scheduler
pub struct UpdateScheduler {
    schedules: Arc<RwLock<HashMap<String, UpdateSchedule>>>,
    storage_path: String,
}

impl UpdateScheduler {
    /// Create a new update scheduler
    pub fn new(storage_path: impl Into<String>) -> Self {
        let storage_path = storage_path.into();
        let schedules = Self::load_schedules(&storage_path);

        Self {
            schedules: Arc::new(RwLock::new(schedules)),
            storage_path,
        }
    }

    /// Schedule an update
    #[allow(clippy::too_many_arguments)]
    pub async fn schedule_update(
        &self,
        source_url: String,
        expected_sha256: Option<String>,
        scheduled_at: Option<DateTime<Utc>>,
        maintenance_window_secs: Option<u32>,
        enable_auto_rollback: bool,
        health_check_timeout_secs: Option<u32>,
        pre_update_hook: Option<String>,
        post_update_hook: Option<String>,
        // Delta params
        is_delta: bool,
        fallback_to_full: bool,
        full_image_url: Option<String>,
    ) -> Result<UpdateSchedule, String> {
        let schedule = UpdateSchedule {
            id: Uuid::new_v4().to_string(),
            source_url,
            expected_sha256,
            scheduled_at,
            maintenance_window_secs,
            enable_auto_rollback,
            health_check_timeout_secs,
            rollback_triggered: false,
            rollback_reason: None,
            pre_update_hook,
            post_update_hook,
            is_delta,
            fallback_to_full,
            full_image_url,
            status: ScheduleStatus::Pending,
            created_at: Utc::now(),
            started_at: None,
            completed_at: None,
            error_message: None,
        };

        info!(
            schedule_id = %schedule.id,
            scheduled_at = ?schedule.scheduled_at,
            "Created update schedule"
        );

        let mut schedules = self.schedules.write().await;
        schedules.insert(schedule.id.clone(), schedule.clone());
        drop(schedules);

        self.persist_schedules().await?;

        Ok(schedule)
    }

    /// Get all schedules
    pub async fn get_schedules(&self) -> Vec<UpdateSchedule> {
        let schedules = self.schedules.read().await;
        schedules.values().cloned().collect()
    }

    /// Get a specific schedule by ID
    #[allow(dead_code)]
    pub async fn get_schedule(&self, id: &str) -> Option<UpdateSchedule> {
        let schedules = self.schedules.read().await;
        schedules.get(id).cloned()
    }

    /// Mark the latest completed or running schedule as `RolledBack`
    ///
    /// Finds the most recently started schedule in `Completed` or `Running` status
    /// and transitions it to `RolledBack` with the given reason.
    pub async fn register_rollback(&self, reason: &str) -> Result<(), String> {
        let mut schedules = self.schedules.write().await;

        // Find the most recently started schedule in Completed or Running status
        let target_id = schedules
            .values()
            .filter(|s| {
                s.status == ScheduleStatus::Completed || s.status == ScheduleStatus::Running
            })
            .max_by_key(|s| s.started_at)
            .map(|s| s.id.clone());

        if let Some(id) = target_id {
            if let Some(schedule) = schedules.get_mut(&id) {
                schedule.rollback_triggered = true;
                schedule.rollback_reason = Some(reason.to_string());
                schedule.status = ScheduleStatus::RolledBack;
                schedule.completed_at = Some(Utc::now());
                info!(schedule_id = %id, reason = %reason, "Registered rollback for schedule");
            }
            drop(schedules);
            self.persist_schedules().await?;
        } else {
            info!(reason = %reason, "No recent schedule found for rollback registration");
        }

        Ok(())
    }

    /// Get the most recently completed or running schedule
    ///
    /// Used by the rollback supervisor to check `enable_auto_rollback` on the
    /// latest update.
    pub async fn get_latest_active_schedule(&self) -> Option<UpdateSchedule> {
        let schedules = self.schedules.read().await;
        schedules
            .values()
            .filter(|s| {
                s.status == ScheduleStatus::Completed || s.status == ScheduleStatus::Running
            })
            .max_by_key(|s| s.started_at)
            .cloned()
    }

    /// Check whether a schedule is still within its maintenance window
    ///
    /// Returns `true` if the schedule has no maintenance window configured or if
    /// the current time falls within `scheduled_at + maintenance_window_secs`.
    pub fn is_within_maintenance_window(schedule: &UpdateSchedule) -> bool {
        let Some(scheduled_at) = schedule.scheduled_at else {
            return true; // No scheduled time means execute immediately
        };

        let Some(window_secs) = schedule.maintenance_window_secs else {
            return true; // No maintenance window means no restriction
        };

        if window_secs == 0 {
            return true;
        }

        let window_end = scheduled_at + chrono::Duration::seconds(i64::from(window_secs));
        let now = Utc::now();

        now <= window_end
    }

    /// Cancel a scheduled update
    pub async fn cancel_schedule(&self, id: &str) -> Result<(), String> {
        let mut schedules = self.schedules.write().await;

        if let Some(schedule) = schedules.get_mut(id) {
            if schedule.status == ScheduleStatus::Pending {
                schedule.status = ScheduleStatus::Cancelled;
                info!(schedule_id = %id, "Cancelled update schedule");
                drop(schedules);
                self.persist_schedules().await?;
                Ok(())
            } else {
                Err(format!(
                    "Cannot cancel schedule in status: {}",
                    schedule.status
                ))
            }
        } else {
            Err(format!("Schedule not found: {}", id))
        }
    }

    /// Update schedule status
    pub async fn update_status(
        &self,
        id: &str,
        status: ScheduleStatus,
        error_message: Option<String>,
    ) -> Result<(), String> {
        let mut schedules = self.schedules.write().await;

        if let Some(schedule) = schedules.get_mut(id) {
            schedule.status = status.clone();
            schedule.error_message = error_message;

            match status {
                ScheduleStatus::Running => {
                    schedule.started_at = Some(Utc::now());
                }
                ScheduleStatus::Completed | ScheduleStatus::Failed | ScheduleStatus::Cancelled => {
                    schedule.completed_at = Some(Utc::now());
                }
                _ => {}
            }

            debug!(schedule_id = %id, status = %schedule.status, "Updated schedule status");
            drop(schedules);
            self.persist_schedules().await?;
            Ok(())
        } else {
            Err(format!("Schedule not found: {}", id))
        }
    }

    /// Get pending schedules that should run now
    pub async fn get_due_schedules(&self) -> Vec<UpdateSchedule> {
        let now = Utc::now();
        let schedules = self.schedules.read().await;

        schedules
            .values()
            .filter(|s| {
                s.status == ScheduleStatus::Pending
                    && s.scheduled_at.is_some_and(|scheduled| scheduled <= now)
            })
            .cloned()
            .collect()
    }

    /// Trigger rollback for a schedule
    #[allow(dead_code)]
    pub async fn trigger_rollback(
        &self,
        id: &str,
        reason: impl Into<String>,
    ) -> Result<(), String> {
        let reason = reason.into();
        let mut schedules = self.schedules.write().await;

        if let Some(schedule) = schedules.get_mut(id) {
            schedule.rollback_triggered = true;
            schedule.rollback_reason = Some(reason.clone());
            schedule.status = ScheduleStatus::RolledBack;
            schedule.completed_at = Some(Utc::now());

            info!(schedule_id = %id, reason = %reason, "Triggered rollback for schedule");
            drop(schedules);
            self.persist_schedules().await?;
            Ok(())
        } else {
            Err(format!("Schedule not found: {}", id))
        }
    }

    /// Persist schedules to disk
    async fn persist_schedules(&self) -> Result<(), String> {
        let schedules = self.schedules.read().await;
        let json = serde_json::to_string_pretty(&*schedules)
            .map_err(|e| format!("Failed to serialize schedules: {}", e))?;

        // Ensure parent directory exists
        if let Some(parent) = Path::new(&self.storage_path).parent() {
            if let Err(e) = fs::create_dir_all(parent) {
                warn!(error = %e, "Failed to create storage directory");
            }
        }

        fs::write(&self.storage_path, json)
            .map_err(|e| format!("Failed to write schedules: {}", e))?;

        debug!(path = %self.storage_path, "Persisted schedules");
        Ok(())
    }

    /// Load schedules from disk
    fn load_schedules(storage_path: &str) -> HashMap<String, UpdateSchedule> {
        match fs::read_to_string(storage_path) {
            Ok(json) => match serde_json::from_str(&json) {
                Ok(schedules) => {
                    info!(path = %storage_path, "Loaded schedules from disk");
                    schedules
                }
                Err(e) => {
                    warn!(error = %e, "Failed to parse schedules, starting fresh");
                    HashMap::new()
                }
            },
            Err(_) => {
                debug!("No existing schedules found, starting fresh");
                HashMap::new()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_schedule_update() {
        let scheduler = UpdateScheduler::new("/tmp/test-schedules.json");

        let schedule = scheduler
            .schedule_update(
                "http://example.com/update.squashfs".to_string(),
                Some("abc123".to_string()),
                Some(Utc::now()),
                None,
                true,
                None,  // health_check_timeout_secs
                None,  // pre_update_hook
                None,  // post_update_hook
                false, // is_delta
                false, // fallback_to_full
                None,  // full_image_url
            )
            .await
            .unwrap();

        assert_eq!(schedule.status, ScheduleStatus::Pending);
        assert!(schedule.enable_auto_rollback);

        // Cleanup
        let _ = fs::remove_file("/tmp/test-schedules.json");
    }

    #[tokio::test]
    async fn test_cancel_schedule() {
        let scheduler = UpdateScheduler::new("/tmp/test-cancel-schedules.json");

        let schedule = scheduler
            .schedule_update(
                "http://example.com/update.squashfs".to_string(),
                None,
                Some(Utc::now() + chrono::Duration::hours(1)),
                Some(3600),
                false,
                None,  // health_check_timeout_secs
                None,  // pre_update_hook
                None,  // post_update_hook
                false, // is_delta
                false, // fallback_to_full
                None,  // full_image_url
            )
            .await
            .unwrap();

        scheduler.cancel_schedule(&schedule.id).await.unwrap();

        let cancelled = scheduler.get_schedule(&schedule.id).await.unwrap();
        assert_eq!(cancelled.status, ScheduleStatus::Cancelled);

        // Cleanup
        let _ = fs::remove_file("/tmp/test-cancel-schedules.json");
    }

    #[tokio::test]
    async fn test_register_rollback() {
        let scheduler = UpdateScheduler::new("/tmp/test-register-rollback.json");

        let schedule = scheduler
            .schedule_update(
                "http://example.com/update.squashfs".to_string(),
                None,
                Some(Utc::now()),
                None,
                true,
                None,
                None,
                None,
                false,
                false,
                None,
            )
            .await
            .unwrap();

        // Mark as running then completed
        scheduler
            .update_status(&schedule.id, ScheduleStatus::Running, None)
            .await
            .unwrap();
        scheduler
            .update_status(&schedule.id, ScheduleStatus::Completed, None)
            .await
            .unwrap();

        // Register rollback
        scheduler
            .register_rollback("Health check failure")
            .await
            .unwrap();

        let updated = scheduler.get_schedule(&schedule.id).await.unwrap();
        assert_eq!(updated.status, ScheduleStatus::RolledBack);
        assert!(updated.rollback_triggered);
        assert_eq!(
            updated.rollback_reason.as_deref(),
            Some("Health check failure")
        );

        // Cleanup
        let _ = fs::remove_file("/tmp/test-register-rollback.json");
    }

    #[tokio::test]
    async fn test_register_rollback_no_schedule() {
        let scheduler = UpdateScheduler::new("/tmp/test-register-rollback-none.json");

        // No schedules exist; should succeed without error
        scheduler.register_rollback("Some reason").await.unwrap();

        // Cleanup
        let _ = fs::remove_file("/tmp/test-register-rollback-none.json");
    }

    #[tokio::test]
    async fn test_get_latest_active_schedule() {
        let scheduler = UpdateScheduler::new("/tmp/test-latest-active.json");

        let s1 = scheduler
            .schedule_update(
                "http://example.com/v1.squashfs".to_string(),
                None,
                Some(Utc::now()),
                None,
                false,
                None,
                None,
                None,
                false,
                false,
                None,
            )
            .await
            .unwrap();

        let s2 = scheduler
            .schedule_update(
                "http://example.com/v2.squashfs".to_string(),
                None,
                Some(Utc::now()),
                None,
                true,
                Some(120),
                None,
                None,
                false,
                false,
                None,
            )
            .await
            .unwrap();

        // No completed schedules yet
        assert!(scheduler.get_latest_active_schedule().await.is_none());

        // Complete s1
        scheduler
            .update_status(&s1.id, ScheduleStatus::Running, None)
            .await
            .unwrap();
        scheduler
            .update_status(&s1.id, ScheduleStatus::Completed, None)
            .await
            .unwrap();

        // Complete s2 (later)
        scheduler
            .update_status(&s2.id, ScheduleStatus::Running, None)
            .await
            .unwrap();
        scheduler
            .update_status(&s2.id, ScheduleStatus::Completed, None)
            .await
            .unwrap();

        // Latest should be s2
        let latest = scheduler.get_latest_active_schedule().await.unwrap();
        assert_eq!(latest.id, s2.id);
        assert!(latest.enable_auto_rollback);

        // Cleanup
        let _ = fs::remove_file("/tmp/test-latest-active.json");
    }

    #[test]
    fn test_maintenance_window_no_schedule_time() {
        let schedule = UpdateSchedule {
            id: "test".to_string(),
            source_url: String::new(),
            expected_sha256: None,
            scheduled_at: None,
            maintenance_window_secs: Some(3600),
            enable_auto_rollback: false,
            health_check_timeout_secs: None,
            pre_update_hook: None,
            post_update_hook: None,
            is_delta: false,
            fallback_to_full: false,
            full_image_url: None,
            rollback_triggered: false,
            rollback_reason: None,
            status: ScheduleStatus::Pending,
            created_at: Utc::now(),
            started_at: None,
            completed_at: None,
            error_message: None,
        };

        // No scheduled_at means always within window
        assert!(UpdateScheduler::is_within_maintenance_window(&schedule));
    }

    #[test]
    fn test_maintenance_window_no_window_configured() {
        let schedule = UpdateSchedule {
            id: "test".to_string(),
            source_url: String::new(),
            expected_sha256: None,
            scheduled_at: Some(Utc::now() - chrono::Duration::hours(2)),
            maintenance_window_secs: None,
            enable_auto_rollback: false,
            health_check_timeout_secs: None,
            pre_update_hook: None,
            post_update_hook: None,
            is_delta: false,
            fallback_to_full: false,
            full_image_url: None,
            rollback_triggered: false,
            rollback_reason: None,
            status: ScheduleStatus::Pending,
            created_at: Utc::now(),
            started_at: None,
            completed_at: None,
            error_message: None,
        };

        // No maintenance_window_secs means no restriction
        assert!(UpdateScheduler::is_within_maintenance_window(&schedule));
    }

    #[test]
    fn test_maintenance_window_within() {
        let schedule = UpdateSchedule {
            id: "test".to_string(),
            source_url: String::new(),
            expected_sha256: None,
            scheduled_at: Some(Utc::now() - chrono::Duration::minutes(10)),
            maintenance_window_secs: Some(3600),
            enable_auto_rollback: false,
            health_check_timeout_secs: None,
            pre_update_hook: None,
            post_update_hook: None,
            is_delta: false,
            fallback_to_full: false,
            full_image_url: None,
            rollback_triggered: false,
            rollback_reason: None,
            status: ScheduleStatus::Pending,
            created_at: Utc::now(),
            started_at: None,
            completed_at: None,
            error_message: None,
        };

        // 10 minutes into a 1-hour window
        assert!(UpdateScheduler::is_within_maintenance_window(&schedule));
    }

    #[test]
    fn test_maintenance_window_expired() {
        let schedule = UpdateSchedule {
            id: "test".to_string(),
            source_url: String::new(),
            expected_sha256: None,
            scheduled_at: Some(Utc::now() - chrono::Duration::hours(2)),
            maintenance_window_secs: Some(3600),
            enable_auto_rollback: false,
            health_check_timeout_secs: None,
            pre_update_hook: None,
            post_update_hook: None,
            is_delta: false,
            fallback_to_full: false,
            full_image_url: None,
            rollback_triggered: false,
            rollback_reason: None,
            status: ScheduleStatus::Pending,
            created_at: Utc::now(),
            started_at: None,
            completed_at: None,
            error_message: None,
        };

        // 2 hours past a 1-hour window
        assert!(!UpdateScheduler::is_within_maintenance_window(&schedule));
    }
}
