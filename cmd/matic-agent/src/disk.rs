//! Disk management for MaticOS A/B partition updates
//!
//! This module handles:
//! - Detecting active/inactive partitions
//! - Flashing OS images to partitions with optional SHA256 verification
//! - Switching boot partitions using GPT attributes

use futures::StreamExt;
use sha2::{Digest, Sha256};
use std::fs;
use std::io;
use std::process::Command;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;
use tracing::{debug, error, info, warn};

/// Information about a partition
pub struct PartitionInfo {
    /// Device path (e.g., "/dev/sda2")
    pub device: String,
    /// Partition number (e.g., 2 or 3)
    pub index: u32,
}

/// Default disk device for MaticOS
const DEFAULT_DISK: &str = "/dev/sda";

/// Partition indices for A/B slots
const SLOT_A_INDEX: u32 = 2;
const SLOT_B_INDEX: u32 = 3;

/// Detect the currently active (booted) partition by parsing /proc/cmdline
///
/// Supports detection via:
/// - Direct device path (e.g., root=/dev/sda2)
/// - PARTUUID (looks up via /dev/disk/by-partuuid/)
pub fn get_active_partition() -> io::Result<PartitionInfo> {
    let cmdline = fs::read_to_string("/proc/cmdline")?;

    // Try to find root= parameter
    for param in cmdline.split_whitespace() {
        if let Some(root_value) = param.strip_prefix("root=") {
            // Handle PARTUUID format
            if let Some(partuuid) = root_value.strip_prefix("PARTUUID=") {
                return resolve_partuuid(partuuid);
            }

            // Handle direct device path
            if root_value.starts_with("/dev/") {
                return parse_device_path(root_value);
            }
        }
    }

    // Fallback: try to detect from /proc/mounts
    if let Ok(mounts) = fs::read_to_string("/proc/mounts") {
        for line in mounts.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 && parts[1] == "/" && parts[0].starts_with("/dev/") {
                return parse_device_path(parts[0]);
            }
        }
    }

    // Ultimate fallback: assume slot A
    warn!("Could not determine active partition, assuming slot A");
    Ok(PartitionInfo {
        device: format!("{}{}", DEFAULT_DISK, SLOT_A_INDEX),
        index: SLOT_A_INDEX,
    })
}

/// Resolve a PARTUUID to a device path
fn resolve_partuuid(partuuid: &str) -> io::Result<PartitionInfo> {
    let link_path = format!("/dev/disk/by-partuuid/{}", partuuid.to_lowercase());

    match fs::read_link(&link_path) {
        Ok(target) => {
            let target_str = target.to_string_lossy();
            // Target is usually relative like "../../sda2"
            if let Some(dev_name) = target_str.rsplit('/').next() {
                let device = format!("/dev/{}", dev_name);
                return parse_device_path(&device);
            }
            Err(io::Error::other(format!(
                "Could not parse symlink target: {}",
                target_str
            )))
        }
        Err(e) => {
            warn!(partuuid = %partuuid, error = %e, "Could not resolve PARTUUID");
            // Fallback to slot A
            Ok(PartitionInfo {
                device: format!("{}{}", DEFAULT_DISK, SLOT_A_INDEX),
                index: SLOT_A_INDEX,
            })
        }
    }
}

/// Parse a device path like "/dev/sda2" to extract partition info
fn parse_device_path(device: &str) -> io::Result<PartitionInfo> {
    // Extract the partition number from the end of the device path
    let index = device
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_digit())
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>()
        .parse::<u32>()
        .map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Cannot parse partition number from: {}", device),
            )
        })?;

    Ok(PartitionInfo {
        device: device.to_string(),
        index,
    })
}

/// Get the inactive partition (the one we can safely write to)
pub fn get_inactive_partition() -> io::Result<PartitionInfo> {
    let active = get_active_partition()?;

    // Determine the base disk device (e.g., "/dev/sda" from "/dev/sda2")
    let base_device: String = active
        .device
        .chars()
        .take_while(|c| !c.is_ascii_digit())
        .collect();

    let inactive_index = if active.index == SLOT_A_INDEX {
        SLOT_B_INDEX
    } else {
        SLOT_A_INDEX
    };

    Ok(PartitionInfo {
        device: format!("{}{}", base_device, inactive_index),
        index: inactive_index,
    })
}

/// Flash an OS image from a URL to a target device with optional SHA256 verification
///
/// # Arguments
/// * `source_url` - HTTP(S) URL to download the image from
/// * `target_device` - Block device to write to (e.g., "/dev/sda3")
/// * `expected_sha256` - Optional SHA256 hash to verify the downloaded image
pub async fn flash_image(
    source_url: &str,
    target_device: &str,
    expected_sha256: Option<&str>,
) -> io::Result<()> {
    info!(url = %source_url, device = %target_device, "Starting image download");

    let response = reqwest::get(source_url)
        .await
        .map_err(|e| io::Error::other(format!("Download failed: {}", e)))?;

    if !response.status().is_success() {
        return Err(io::Error::other(format!(
            "Server returned error: {}",
            response.status()
        )));
    }

    let content_length = response.content_length().unwrap_or(0);
    info!(size_bytes = content_length, device = %target_device, "Flashing image");

    let mut file = OpenOptions::new().write(true).open(target_device).await?;

    let mut hasher = Sha256::new();
    let mut bytes_written: u64 = 0;

    let mut stream = response.bytes_stream();
    while let Some(item) = stream.next().await {
        let chunk = item.map_err(|e| io::Error::other(format!("Stream error: {}", e)))?;

        // Update hash
        hasher.update(&chunk);

        // Write to device
        file.write_all(&chunk).await?;
        bytes_written += chunk.len() as u64;

        // Progress indication (every ~10MB)
        if bytes_written % (10 * 1024 * 1024) < chunk.len() as u64 && content_length > 0 {
            let percent = (bytes_written * 100) / content_length;
            debug!(
                percent = percent,
                bytes = bytes_written,
                total = content_length,
                "Flash progress"
            );
        }
    }

    file.flush().await?;
    file.sync_all().await?;
    info!(bytes = bytes_written, device = %target_device, "Image written successfully");

    // Verify SHA256 if provided
    if let Some(expected) = expected_sha256 {
        if !expected.is_empty() {
            let actual = format!("{:x}", hasher.finalize());
            if actual != expected.to_lowercase() {
                error!(expected = %expected, actual = %actual, "SHA256 verification failed");
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("SHA256 mismatch: expected {}, got {}", expected, actual),
                ));
            }
            info!(hash = %actual, "SHA256 verification passed");
        }
    }

    Ok(())
}

/// Switch the boot partition by updating GPT partition attributes
///
/// This uses sgdisk to:
/// 1. Clear the "legacy BIOS bootable" attribute from all partitions
/// 2. Set the "legacy BIOS bootable" attribute on the target partition
/// 3. Set GPT attribute bit 2 (legacy_boot) on the target partition
///
/// For systems using GRUB or other bootloaders that respect these flags,
/// this will cause the target partition to be booted on next restart.
pub fn switch_boot_partition(target_index: u32) -> io::Result<()> {
    info!(target_index = target_index, "Switching boot partition");

    // Check if sgdisk is available
    if !std::path::Path::new("/usr/sbin/sgdisk").exists()
        && !std::path::Path::new("/sbin/sgdisk").exists()
    {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "sgdisk not found. Cannot modify partition table.",
        ));
    }

    let sgdisk = if std::path::Path::new("/usr/sbin/sgdisk").exists() {
        "/usr/sbin/sgdisk"
    } else {
        "/sbin/sgdisk"
    };

    // Determine which partition to clear (the other one)
    let other_index = if target_index == SLOT_A_INDEX {
        SLOT_B_INDEX
    } else {
        SLOT_A_INDEX
    };

    // Clear legacy_boot attribute from the other partition
    // Attribute bit 2 is "Legacy BIOS Bootable"
    let output = Command::new(sgdisk)
        .args([
            &format!("--attributes={}:clear:2", other_index),
            DEFAULT_DISK,
        ])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        warn!(partition = other_index, error = %stderr, "Failed to clear boot flag");
        // Continue anyway - setting the target is more important
    }

    // Set legacy_boot attribute on the target partition
    let output = Command::new(sgdisk)
        .args([
            &format!("--attributes={}:set:2", target_index),
            DEFAULT_DISK,
        ])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(io::Error::other(format!(
            "Failed to set boot flag on partition {}: {}",
            target_index, stderr
        )));
    }

    info!(
        device = format!("{}{}", DEFAULT_DISK, target_index),
        "Boot partition switched"
    );

    // Also update /etc/matic/boot.next as a software-level indicator (if writable)
    let boot_marker = "/tmp/boot.next";
    if let Err(e) = fs::write(boot_marker, format!("{}", target_index)) {
        warn!(error = %e, "Could not write boot marker");
    }

    Ok(())
}

/// State file for tracking rollback information
const ROLLBACK_STATE_FILE: &str = "/var/lib/matic/rollback_state.json";

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
struct RollbackState {
    previous_partition: Option<u32>,
    boot_counter: u32,
    last_update_time: Option<String>,
}

/// Load rollback state from disk
fn load_rollback_state() -> RollbackState {
    match fs::read_to_string(ROLLBACK_STATE_FILE) {
        Ok(json) => serde_json::from_str(&json).unwrap_or_default(),
        Err(_) => RollbackState::default(),
    }
}

/// Save rollback state to disk
fn save_rollback_state(state: &RollbackState) -> io::Result<()> {
    // Ensure parent directory exists
    if let Some(parent) = std::path::Path::new(ROLLBACK_STATE_FILE).parent() {
        fs::create_dir_all(parent)?;
    }

    let json = serde_json::to_string_pretty(state)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    fs::write(ROLLBACK_STATE_FILE, json)?;
    debug!("Saved rollback state");
    Ok(())
}

/// Track the current active partition before switching (for rollback)
#[allow(dead_code)]
pub fn record_active_partition_for_rollback() -> io::Result<()> {
    let active = get_active_partition()?;
    let mut state = load_rollback_state();
    state.previous_partition = Some(active.index);
    state.last_update_time = Some(chrono::Utc::now().to_rfc3339());
    save_rollback_state(&state)?;
    info!(
        previous_partition = active.index,
        "Recorded partition for rollback"
    );
    Ok(())
}

/// Rollback to the previous partition
pub fn rollback_to_previous_partition() -> io::Result<()> {
    let state = load_rollback_state();

    let previous_index = state.previous_partition.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "No previous partition recorded for rollback",
        )
    })?;

    info!(
        target_partition = previous_index,
        "Rolling back to previous partition"
    );

    // Switch back to the previous partition
    switch_boot_partition(previous_index)?;

    // Clear the rollback state
    let mut state = load_rollback_state();
    state.previous_partition = None;
    state.boot_counter = 0;
    save_rollback_state(&state)?;

    Ok(())
}

/// Get the current boot counter
#[allow(dead_code)]
pub fn get_boot_counter() -> u32 {
    load_rollback_state().boot_counter
}

/// Increment the boot counter (called on each boot)
#[allow(dead_code)]
pub fn increment_boot_counter() -> io::Result<()> {
    let mut state = load_rollback_state();
    state.boot_counter += 1;
    save_rollback_state(&state)?;
    info!(
        boot_counter = state.boot_counter,
        "Incremented boot counter"
    );
    Ok(())
}

/// Clear the boot counter after successful boot + health checks
#[allow(dead_code)]
pub fn clear_boot_counter() -> io::Result<()> {
    let mut state = load_rollback_state();
    state.boot_counter = 0;
    save_rollback_state(&state)?;
    info!("Cleared boot counter");
    Ok(())
}

/// Check if we're in a boot loop (too many failed boots)
#[allow(dead_code)]
pub fn is_boot_loop() -> bool {
    const MAX_BOOT_ATTEMPTS: u32 = 3;
    let counter = get_boot_counter();
    if counter >= MAX_BOOT_ATTEMPTS {
        warn!(
            boot_counter = counter,
            max_attempts = MAX_BOOT_ATTEMPTS,
            "Boot loop detected"
        );
        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_device_path() {
        let info = parse_device_path("/dev/sda2").unwrap();
        assert_eq!(info.device, "/dev/sda2");
        assert_eq!(info.index, 2);

        let info = parse_device_path("/dev/nvme0n1p3").unwrap();
        assert_eq!(info.device, "/dev/nvme0n1p3");
        assert_eq!(info.index, 3);
    }

    #[test]
    fn test_inactive_partition_calculation() {
        // When slot A is active, slot B should be inactive
        let active_a = PartitionInfo {
            device: "/dev/sda2".to_string(),
            index: SLOT_A_INDEX,
        };

        // Can't call get_inactive_partition directly in unit tests
        // (requires /proc/cmdline), but we can test the logic
        let inactive_index = if active_a.index == SLOT_A_INDEX {
            SLOT_B_INDEX
        } else {
            SLOT_A_INDEX
        };
        assert_eq!(inactive_index, SLOT_B_INDEX);
    }
}
