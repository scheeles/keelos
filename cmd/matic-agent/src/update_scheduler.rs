//! Update scheduler for MaticOS
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
use tracing::{debug, error, info, warn};
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
}

impl ToString for ScheduleStatus {
    fn to_string(&self) -> String {
        match self {
            ScheduleStatus::Pending => "pending".to_string(),
            ScheduleStatus::Running => "running".to_string(),
            ScheduleStatus::Completed => "completed".to_string(),
            ScheduleStatus::Failed => "failed".to_string(),
            ScheduleStatus::Cancelled => "cancelled".to_string(),
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
    pub async fn schedule_update(
        &self,
        source_url: String,
        expected_sha256: Option<String>,
        scheduled_at: Option<DateTime<Utc>>,
        maintenance_window_secs: Option<u32>,
        enable_auto_rollback: bool,
        pre_update_hook: Option<String>,
        post_update_hook: Option<String>,
    ) -> Result<UpdateSchedule, String> {
        let schedule = UpdateSchedule {
            id: Uuid::new_v4().to_string(),
            source_url,
            expected_sha256,
            scheduled_at,
            maintenance_window_secs,
            enable_auto_rollback,
            pre_update_hook,
            post_update_hook,
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
    pub async fn get_schedule(&self, id: &str) -> Option<UpdateSchedule> {
        let schedules = self.schedules.read().await;
        schedules.get(id).cloned()
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
                    schedule.status.to_string()
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

            debug!(schedule_id = %id, status = %schedule.status.to_string(), "Updated schedule status");
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
                    && s.scheduled_at.map_or(false, |scheduled| scheduled <= now)
            })
            .cloned()
            .collect()
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
                None,
                None,
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
                None,
                None,
                false,
                None,
                None,
            )
            .await
            .unwrap();

        scheduler.cancel_schedule(&schedule.id).await.unwrap();

        let cancelled = scheduler.get_schedule(&schedule.id).await.unwrap();
        assert_eq!(cancelled.status, ScheduleStatus::Cancelled);

        // Cleanup
        let _ = fs::remove_file("/tmp/test-cancel-schedules.json");
    }
}
