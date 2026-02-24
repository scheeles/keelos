# API Reference

The KeelOS API is defined using Protocol Buffers (v3). Services are exposed on port `50051` by default.

## Service: `NodeService`

The core service for node management.

### RPC Methods

#### `GetStatus`
Returns static information about the node.
*   **Request**: `GetStatusRequest` (Empty)
*   **Response**: `GetStatusResponse`
    *   `hostname` (string)
    *   `kernel_version` (string)
    *   `os_version` (string)
    *   `uptime_seconds` (float)

#### `GetHealth`
Returns dynamic health status.
*   **Request**: `GetHealthRequest` (Empty)
*   **Response**: `GetHealthResponse`
    *   `status` (string): "healthy", "degraded", "unhealthy"
    *   `checks` (repeated `HealthCheckResult`): List of specific checks.

#### `InstallUpdate`
Streams the installation of an update.
*   **Request**: `InstallUpdateRequest`
    *   `source_url` (string): URL/Path to image.
    *   `expected_sha256` (string): Checksum for verification.
*   **Response**: (Stream) `UpdateProgress`
    *   `percentage` (int): 0-100.
    *   `message` (string): Current step description.

#### `ScheduleUpdate`
Schedules an update operation.
*   **Request**: `ScheduleUpdateRequest`
    *   `source_url` (string)
    *   `scheduled_at` (string): RFC3339 timestamp.
    *   `enable_auto_rollback` (bool): If true, enables watchdog fallback.
    *   `health_check_timeout_secs` (int): Time to wait for health before rolling back.

#### `Reboot`
Safely reboots the machine.
*   **Request**: `RebootRequest`
    *   `reason` (string): Audit log reason.

#### `TriggerRollback`
Manually reverts to the previous partition.
*   **Request**: `TriggerRollbackRequest`
    *   `reason` (string)
*   **Response**: `TriggerRollbackResponse`
    *   `success` (bool)
    *   `message` (string)

#### `GetRollbackHistory`
Returns a list of past rollback events.
*   **Response**: `GetRollbackHistoryResponse`
    *   `events` (repeated `RollbackEvent`)

### Diagnostics & Debugging

For complete diagnostics API documentation, see [Diagnostics API Reference](./diagnostics-api.md).

#### `EnableDebugMode`
Activates a time-limited debug session.
*   **Request**: `EnableDebugModeRequest` — `duration_secs`, `reason`
*   **Response**: `EnableDebugModeResponse` — `success`, `session_id`, `expires_at`

#### `GetDebugStatus`
Returns current debug session status.
*   **Response**: `GetDebugStatusResponse` — `enabled`, `session_id`, `remaining_secs`

#### `CollectCrashDump`
Collects kernel and userspace diagnostic data.
*   **Request**: `CollectCrashDumpRequest` — `include_kernel`, `include_userspace`
*   **Response**: `CollectCrashDumpResponse` — `dump_path`, `dump_size_bytes`

#### `StreamLogs`
Streams system logs with level and component filtering.
*   **Request**: `StreamLogsRequest` — `level`, `component`, `tail_lines`
*   **Response**: (Stream) `LogEntry` — `timestamp`, `level`, `component`, `message`

#### `CreateSystemSnapshot`
Creates a point-in-time system state capture.
*   **Request**: `CreateSystemSnapshotRequest` — `label`, `include_config`, `include_logs`
*   **Response**: `CreateSystemSnapshotResponse` — `snapshot_id`, `snapshot_path`, `size_bytes`

#### `EnableRecoveryMode`
Enables time-limited emergency recovery mode.
*   **Request**: `EnableRecoveryModeRequest` — `reason`, `duration_secs`
*   **Response**: `EnableRecoveryModeResponse` — `success`, `expires_at`
