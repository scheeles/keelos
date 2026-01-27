//! Boot-time telemetry tracking for keel-init
//!
//! This module tracks boot phase durations and exports metrics
//! for the agent to report.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tracing::info;

/// Boot phase metrics
#[derive(Debug, Serialize, Deserialize)]
pub struct BootMetrics {
    /// Total boot time in seconds
    pub total_boot_time_secs: f64,
    /// Individual phase durations
    pub phases: HashMap<String, f64>,
}

/// Boot phase tracker
pub struct BootPhaseTracker {
    start_time: Instant,
    phases: HashMap<String, Duration>,
    current_phase: Option<(String, Instant)>,
}

impl BootPhaseTracker {
    /// Create a new boot phase tracker
    pub fn new() -> Self {
        Self {
            start_time: Instant::now(),
            phases: HashMap::new(),
            current_phase: None,
        }
    }

    /// Start tracking a new phase
    pub fn start_phase(&mut self, name: impl Into<String>) {
        let name = name.into();

        // End current phase if any
        if let Some((prev_name, prev_start)) = self.current_phase.take() {
            let duration = prev_start.elapsed();
            self.phases.insert(prev_name.clone(), duration);
            info!(
                phase = %prev_name,
                duration_ms = duration.as_millis(),
                "Boot phase completed"
            );
        }

        // Start new phase
        info!(phase = %name, "Starting boot phase");
        self.current_phase = Some((name, Instant::now()));
    }

    /// End the current phase
    pub fn end_current_phase(&mut self) {
        if let Some((name, start)) = self.current_phase.take() {
            let duration = start.elapsed();
            self.phases.insert(name.clone(), duration);
            info!(
                phase = %name,
                duration_ms = duration.as_millis(),
                "Boot phase completed"
            );
        }
    }

    /// Get boot metrics
    pub fn get_metrics(&self) -> BootMetrics {
        let total_boot_time = self.start_time.elapsed();
        let phases = self
            .phases
            .iter()
            .map(|(k, v)| (k.clone(), v.as_secs_f64()))
            .collect();

        BootMetrics {
            total_boot_time_secs: total_boot_time.as_secs_f64(),
            phases,
        }
    }

    /// Export metrics to JSON file
    pub fn export_to_file(&self, path: &str) -> Result<(), Box<dyn std::error::Error>> {
        let metrics = self.get_metrics();
        let json = serde_json::to_string_pretty(&metrics)?;
        std::fs::write(path, json)?;
        info!(path = %path, "Exported boot metrics");
        Ok(())
    }
}

impl Default for BootPhaseTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_boot_phase_tracker() {
        let mut tracker = BootPhaseTracker::new();

        tracker.start_phase("filesystem");
        thread::sleep(Duration::from_millis(10));

        tracker.start_phase("network");
        thread::sleep(Duration::from_millis(10));

        tracker.end_current_phase();

        let metrics = tracker.get_metrics();
        assert!(metrics.phases.contains_key("filesystem"));
        assert!(metrics.phases.contains_key("network"));
        assert!(metrics.total_boot_time_secs > 0.0);
    }

    #[test]
    fn test_metrics_export() {
        let mut tracker = BootPhaseTracker::new();
        tracker.start_phase("test_phase");
        tracker.end_current_phase();

        let temp_file = "/tmp/test-boot-metrics.json";
        let result = tracker.export_to_file(temp_file);
        assert!(result.is_ok());

        // Cleanup
        let _ = std::fs::remove_file(temp_file);
    }
}
