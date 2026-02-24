# Update Scheduling API

This document describes the gRPC API endpoints for scheduling, listing, and cancelling OS updates.

## Table of Contents

- [ScheduleUpdate](#scheduleupdate)
- [GetUpdateSchedule](#getupdateschedule)
- [CancelScheduledUpdate](#cancelscheduledupdate)
- [Data Types](#data-types)
- [Examples](#examples)

---

## ScheduleUpdate

Creates a new update schedule. The update will be executed automatically when the scheduled time arrives, provided it falls within the maintenance window.

**RPC Method:**
```protobuf
rpc ScheduleUpdate (ScheduleUpdateRequest) returns (ScheduleUpdateResponse);
```

**Request:**
```protobuf
message ScheduleUpdateRequest {
  string source_url = 1;                  // URL of the SquashFS image (or delta file)
  string expected_sha256 = 2;             // Expected SHA256 checksum (optional)
  string scheduled_at = 3;               // RFC3339 timestamp (optional; empty = immediate)
  uint32 maintenance_window_secs = 4;     // Maintenance window duration in seconds (optional)
  bool enable_auto_rollback = 5;          // Enable automatic rollback on health failure
  uint32 health_check_timeout_secs = 6;   // Health check grace period after reboot (optional)
  string pre_update_hook = 7;             // Command to run before the update (optional)
  string post_update_hook = 8;            // Command to run after the update (optional)
  bool is_delta = 9;                      // Whether source_url points to a delta file
  bool fallback_to_full = 10;             // Fall back to full image if delta fails
  string full_image_url = 11;             // Full image URL for delta fallback
}
```

**Response:**
```protobuf
message ScheduleUpdateResponse {
  string schedule_id = 1;   // UUID of the created schedule
  string status = 2;        // Initial status ("pending")
  string scheduled_at = 3;  // Confirmed schedule time (RFC3339)
}
```

**Example Request:**
```json
{
  "source_url": "http://update-server/os-v2.0.squashfs",
  "expected_sha256": "abc123...",
  "scheduled_at": "2026-03-01T02:00:00Z",
  "maintenance_window_secs": 3600,
  "enable_auto_rollback": true,
  "health_check_timeout_secs": 300
}
```

**Example Response:**
```json
{
  "schedule_id": "550e8400-e29b-41d4-a716-446655440000",
  "status": "pending",
  "scheduled_at": "2026-03-01T02:00:00+00:00"
}
```

**Error Codes:**
- `INVALID_ARGUMENT` — Invalid `scheduled_at` timestamp (must be RFC3339 format)
- `INTERNAL` — Failed to persist schedule to storage

---

## GetUpdateSchedule

Retrieves all update schedules (pending, running, completed, failed, cancelled, rolled back).

**RPC Method:**
```protobuf
rpc GetUpdateSchedule (GetUpdateScheduleRequest) returns (GetUpdateScheduleResponse);
```

**Request:**
```protobuf
message GetUpdateScheduleRequest {}
```

**Response:**
```protobuf
message GetUpdateScheduleResponse {
  repeated UpdateSchedule schedules = 1;
}
```

**Example Response:**
```json
{
  "schedules": [
    {
      "id": "550e8400-e29b-41d4-a716-446655440000",
      "source_url": "http://update-server/os-v2.0.squashfs",
      "expected_sha256": "abc123...",
      "scheduled_at": "2026-03-01T02:00:00+00:00",
      "status": "pending",
      "enable_auto_rollback": true,
      "created_at": "2026-02-28T10:00:00+00:00"
    }
  ]
}
```

---

## CancelScheduledUpdate

Cancels a pending update schedule. Only schedules in `pending` status can be cancelled.

**RPC Method:**
```protobuf
rpc CancelScheduledUpdate (CancelScheduledUpdateRequest) returns (CancelScheduledUpdateResponse);
```

**Request:**
```protobuf
message CancelScheduledUpdateRequest {
  string schedule_id = 1;  // UUID of the schedule to cancel
}
```

**Response:**
```protobuf
message CancelScheduledUpdateResponse {
  bool success = 1;    // Whether the cancellation succeeded
  string message = 2;  // Human-readable result message
}
```

**Example Response (Success):**
```json
{
  "success": true,
  "message": "Update cancelled successfully"
}
```

**Example Response (Failure):**
```json
{
  "success": false,
  "message": "Cannot cancel schedule in status: running"
}
```

---

## Data Types

### UpdateSchedule

Represents a single update schedule entry.

```protobuf
message UpdateSchedule {
  string id = 1;                    // UUID
  string source_url = 2;           // Image URL
  string expected_sha256 = 3;      // Expected checksum
  string scheduled_at = 4;         // Scheduled execution time (RFC3339)
  string status = 5;               // Current status
  bool enable_auto_rollback = 6;   // Whether auto-rollback is enabled
  string created_at = 7;           // Creation timestamp (RFC3339)
}
```

**Status Values:**

| Status | Description |
|--------|-------------|
| `pending` | Waiting for the scheduled time to arrive. |
| `running` | Update is currently being applied. |
| `completed` | Update finished successfully. |
| `failed` | Update failed (see error message). |
| `cancelled` | Update was cancelled before execution. |
| `rolled_back` | Update was rolled back after completion. |

### Maintenance Windows

When `maintenance_window_secs` is set, the update will only execute if the current time falls within `scheduled_at + maintenance_window_secs`. If the window expires before the executor picks up the schedule, it is marked as `failed` with the message "Maintenance window expired".

**Example:** A schedule set for `02:00` with a 1-hour window (`3600` seconds) will only execute between `02:00` and `03:00`.

### Auto-Rollback

When `enable_auto_rollback` is `true`, the rollback supervisor runs health checks after the node reboots into the new version. If health checks report `unhealthy`, the system automatically reverts to the previous partition and reboots.

The grace period before health checks begin is controlled by `health_check_timeout_secs` (default: 60 seconds).

---

## Examples

### CLI Examples

**Schedule an update for 2 AM with a 1-hour maintenance window:**
```bash
osctl schedule update \
  --source http://update-server/os-v2.0.squashfs \
  --scheduled-at "2026-03-01T02:00:00Z" \
  --maintenance-window 3600 \
  --enable-auto-rollback \
  --health-check-timeout 300
```

**Schedule a delta update with fallback:**
```bash
osctl schedule update \
  --source http://update-server/delta-v1-to-v2.bin \
  --delta \
  --fallback \
  --full-image-url http://update-server/os-v2.0.squashfs \
  --enable-auto-rollback
```

**List all schedules:**
```bash
osctl schedule list
```

**Cancel a pending schedule:**
```bash
osctl schedule cancel --id 550e8400-e29b-41d4-a716-446655440000
```

### gRPC Examples

**Go:**
```go
// Schedule an update
resp, err := client.ScheduleUpdate(ctx, &node.ScheduleUpdateRequest{
    SourceUrl:              "http://update-server/os-v2.0.squashfs",
    ScheduledAt:            "2026-03-01T02:00:00Z",
    MaintenanceWindowSecs:  3600,
    EnableAutoRollback:     true,
    HealthCheckTimeoutSecs: 300,
})
log.Printf("Scheduled: %s (status: %s)", resp.ScheduleId, resp.Status)

// List schedules
listResp, err := client.GetUpdateSchedule(ctx, &node.GetUpdateScheduleRequest{})
for _, s := range listResp.Schedules {
    log.Printf("[%s] %s — %s", s.Status, s.Id, s.SourceUrl)
}

// Cancel a schedule
cancelResp, err := client.CancelScheduledUpdate(ctx, &node.CancelScheduledUpdateRequest{
    ScheduleId: "550e8400-e29b-41d4-a716-446655440000",
})
log.Printf("Cancelled: %t — %s", cancelResp.Success, cancelResp.Message)
```

---

## Related Documentation

- [Health & Rollback API](./health-and-rollback.md)
- [Lifecycle Management](../operational-guides/lifecycle-management.md)
- [Configuration Schema](../reference/configuration.md)
- [osctl CLI Reference](../reference/osctl.md)
