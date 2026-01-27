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
