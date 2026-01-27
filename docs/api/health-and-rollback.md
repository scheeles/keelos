# Health Check and Rollback API

This document describes the gRPC API endpoints for health monitoring and automatic rollback functionality introduced in Phase 2 of the Enhanced Update Mechanism.

## Table of Contents

- [Health Check API](#health-check-api)
- [Rollback API](#rollback-api)
- [Data Types](#data-types)
- [Examples](#examples)

---

## Health Check API

### GetHealth

Retrieves the current health status of the system, including results from all configured health checks.

**RPC Method:**
```protobuf
rpc GetHealth (GetHealthRequest) returns (GetHealthResponse);
```

**Request:**
```protobuf
message GetHealthRequest {}
```

**Response:**
```protobuf
message GetHealthResponse {
  string status = 1;                        // Overall health status
  repeated HealthCheckResult checks = 2;    // Individual check results
  string last_update_time = 3;              // ISO 8601 timestamp
}
```

**Status Values:**
- `healthy` - All critical checks passing
- `degraded` - Non-critical checks failing
- `unhealthy` - Critical checks failing

**Example Response:**
```json
{
  "status": "healthy",
  "last_update_time": "2026-01-27T00:00:00Z",
  "checks": [
    {
      "name": "boot",
      "status": "pass",
      "message": "OK",
      "duration_ms": 45
    },
    {
      "name": "network",
      "status": "pass",
      "message": "OK",
      "duration_ms": 12
    }
  ]
}
```

---

## Rollback API

### TriggerRollback

Manually triggers a rollback to the previous OS partition. This is useful for emergency recovery or when automatic rollback fails.

**RPC Method:**
```protobuf
rpc TriggerRollback (TriggerRollbackRequest) returns (TriggerRollbackResponse);
```

**Request:**
```protobuf
message TriggerRollbackRequest {
  string reason = 1;  // Reason for manual rollback
}
```

**Response:**
```protobuf
message TriggerRollbackResponse {
  bool success = 1;   // Whether rollback was successful
  string message = 2; // Human-readable result message
}
```

**Example Request:**
```json
{
  "reason": "Application crash after update"
}
```

**Example Response (Success):**
```json
{
  "success": true,
  "message": "Rollback completed. System will reboot to previous partition."
}
```

**Example Response (Failure):**
```json
{
  "success": false,
  "message": "Rollback failed: No previous partition recorded for rollback"
}
```

---

### GetRollbackHistory

Retrieves the history of rollback events, both automatic and manual.

**RPC Method:**
```protobuf
rpc GetRollbackHistory (GetRollbackHistoryRequest) returns (GetRollbackHistoryResponse);
```

**Request:**
```protobuf
message GetRollbackHistoryRequest {}
```

**Response:**
```protobuf
message GetRollbackHistoryResponse {
  repeated RollbackEvent events = 1;
}
```

**Example Response:**
```json
{
  "events": [
    {
      "timestamp": "2026-01-26T23:00:00Z",
      "reason": "Health checks failed",
      "from_partition": "3",
      "to_partition": "2",
      "automatic": true
    },
    {
      "timestamp": "2026-01-27T00:00:00Z",
      "reason": "Manual rollback via osctl",
      "from_partition": "3",
      "to_partition": "2",
      "automatic": false
    }
  ]
}
```

---

## Data Types

### HealthCheckResult

Individual health check execution result.

```protobuf
message HealthCheckResult {
  string name = 1;        // Check name (e.g., "boot", "network")
  string status = 2;      // "pass", "fail", or "unknown"
  string message = 3;     // Human-readable result message
  uint64 duration_ms = 4; // Execution duration in milliseconds
}
```

**Built-in Health Checks:**

| Name | Description | Critical | Failure Condition |
|------|-------------|----------|-------------------|
| `boot` | System boot verification | Yes | Uptime < 10 seconds |
| `service` | Service status check | Yes | keel-agent not running |
| `network` | Network connectivity | No | No active interfaces |
| `api` | API responsiveness | Yes | gRPC port not listening |

---

### RollbackEvent

Record of a rollback operation.

```protobuf
message RollbackEvent {
  string timestamp = 1;       // ISO 8601 timestamp
  string reason = 2;          // Reason for rollback
  string from_partition = 3;  // Source partition
  string to_partition = 4;    // Destination partition
  bool automatic = 5;         // True if triggered automatically
}
```

---

## Examples

### CLI Examples

**Check System Health:**
```bash
osctl --endpoint http://node:50051 health
```

**Trigger Manual Rollback:**
```bash
osctl --endpoint http://node:50051 rollback trigger \
  --reason "Application not starting"
```

**View Rollback History:**
```bash
osctl --endpoint http://node:50051 rollback history
```

### gRPC Examples

**Python:**
```python
import grpc
from matic_api.node import node_pb2, node_pb2_grpc

# Create channel
channel = grpc.insecure_channel('localhost:50051')
stub = node_pb2_grpc.NodeServiceStub(channel)

# Get health
response = stub.GetHealth(node_pb2.GetHealthRequest())
print(f"Status: {response.status}")
for check in response.checks:
    print(f"  {check.name}: {check.status} ({check.duration_ms}ms)")

# Trigger rollback
response = stub.TriggerRollback(
    node_pb2.TriggerRollbackRequest(reason="Emergency recovery")
)
print(f"Success: {response.success}, Message: {response.message}")
```

**Go:**
```go
import (
    "context"
    "log"
    "google.golang.org/grpc"
    pb "github.com/scheeles/keelos/pkg/api/node"
)

conn, _ := grpc.Dial("localhost:50051", grpc.WithInsecure())
client := pb.NewNodeServiceClient(conn)

// Get health
health, _ := client.GetHealth(context.Background(), &pb.GetHealthRequest{})
log.Printf("Status: %s", health.Status)

// Trigger rollback
resp, _ := client.TriggerRollback(context.Background(), &pb.TriggerRollbackRequest{
    Reason: "Emergency recovery",
})
log.Printf("Success: %t", resp.Success)
```

---

## Error Handling

### Common Error Scenarios

**No Previous Partition:**
```json
{
  "success": false,
  "message": "Rollback failed: No previous partition recorded for rollback"
}
```
- **Cause**: System has not performed an update yet
- **Resolution**: Rollback is only available after at least one update

**Health Check Timeout:**
- Default timeout: 5 minutes (300 seconds)
- Configurable via `health_check_timeout_secs` in `ScheduleUpdateRequest`
- After timeout, update is marked as failed and rollback triggered

**Boot Loop Detection:**
- Maximum boot attempts: 3
- After 3 failed boots, system enters recovery mode
- Manual intervention required

---

## Configuration

### Health Check Configuration

Health checks can be configured in the keel-agent:

```rust
let health_config = HealthCheckerConfig {
    timeout_secs: 300,      // 5 minutes
    max_retries: 3,         // Retry failed checks 3 times
    retry_delay_ms: 1000,   // 1 second between retries
};
```

### Update Scheduling with Rollback

```bash
osctl schedule update \
  --source http://server/os-v2.0.squashfs \
  --enable-auto-rollback \
  --health-check-timeout 300
```

---

## Security Considerations

1. **Authentication**: Health and rollback APIs respect the same mTLS authentication as other endpoints
2. **Authorization**: Manual rollback should be restricted to privileged users
3. **Audit Logging**: All rollback events are logged with reason and timestamp
4. **Rate Limiting**: Consider rate limiting manual rollback triggers to prevent abuse

---

## Monitoring

### Recommended Metrics

- `health_check_duration_ms` - Time taken for each check
- `health_check_failures_total` - Number of failed checks
- `rollback_events_total` - Total rollback events (by type: automatic/manual)
- `boot_counter` - Current boot attempt number

### Alerting

Recommended alerts:
- Health status == "unhealthy" for > 5 minutes
- Rollback event occurred
- Boot counter > 2 (approaching boot loop)

---

## Related Documentation

- [Enhanced Update Mechanism](../README.md#enhanced-update-mechanism)
- [Update Scheduling API](./update-scheduling.md)
- [Troubleshooting Guide](./troubleshooting.md)
