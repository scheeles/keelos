# Audit Logging Guide

KeelOS automatically records a structured audit trail for every gRPC API operation. Since KeelOS has no SSH or shell access, this built-in audit log provides a tamper-evident record of all management actions performed on the node.

## Overview

| Feature | Details |
|---------|---------|
| **Format** | JSON-lines (one JSON object per line) |
| **Location** | `/var/lib/keel/audit/audit.log` on the node |
| **Scope** | Every gRPC API call (all `osctl` commands) |
| **Fields** | Timestamp, method, status, duration |
| **Activation** | Always enabled — no configuration required |

Audit logging is implemented as a transparent Tower middleware layer. Every request that reaches the `keel-agent` gRPC server is automatically recorded before and after the handler executes, including the response status and latency.

## Log Format

Each line in the audit log is a self-contained JSON object:

```json
{"timestamp":"2025-01-15T14:30:00.123456+00:00","method":"/keel.v1.NodeService/GetStatus","status":"OK","duration_ms":2}
```

### Fields

| Field | Type | Description |
|-------|------|-------------|
| `timestamp` | string | ISO-8601 timestamp of when the request completed |
| `method` | string | Full gRPC method path (e.g. `/keel.v1.NodeService/Reboot`) |
| `status` | string | gRPC status code name (`OK`, `INTERNAL`, `PERMISSION_DENIED`, etc.) |
| `duration_ms` | integer | Request duration in milliseconds |

### Status Codes

The `status` field maps gRPC numeric codes to human-readable names:

| Code | Name |
|------|------|
| 0 | `OK` |
| 3 | `INVALID_ARGUMENT` |
| 5 | `NOT_FOUND` |
| 7 | `PERMISSION_DENIED` |
| 13 | `INTERNAL` |
| 14 | `UNAVAILABLE` |
| 16 | `UNAUTHENTICATED` |

If the gRPC status header is not present, the HTTP status code is recorded instead (e.g. `HTTP 200`).

## Viewing Audit Logs

Since KeelOS is immutable and has no shell access, audit logs are accessed through system snapshots or crash dumps.

### Via System Snapshot

Create a snapshot that includes the audit log:

```bash
osctl --endpoint http://<NODE_IP>:50051 diag snapshot \
  --label "audit-review-2025-01-15"
```

### Via Structured Logging

Audit events are also emitted through the `tracing` structured logging framework. When an OTLP (OpenTelemetry) endpoint is configured, audit entries appear in your centralized logging system with the following fields:

- `message`: `"audit"`
- `method`: gRPC method path
- `status`: Response status
- `duration_ms`: Latency in milliseconds

Configure the OTLP endpoint via the `OTLP_ENDPOINT` environment variable on the agent.

## Example Audit Entries

### Successful Status Check

```json
{"timestamp":"2025-01-15T14:30:00+00:00","method":"/keel.v1.NodeService/GetStatus","status":"OK","duration_ms":1}
```

### OS Update Installation

```json
{"timestamp":"2025-01-15T14:31:00+00:00","method":"/keel.v1.NodeService/InstallUpdate","status":"OK","duration_ms":45230}
```

### Reboot Request

```json
{"timestamp":"2025-01-15T14:32:00+00:00","method":"/keel.v1.NodeService/Reboot","status":"OK","duration_ms":3}
```

### Debug Mode Enabled

```json
{"timestamp":"2025-01-15T14:33:00+00:00","method":"/keel.v1.NodeService/EnableDebugMode","status":"OK","duration_ms":5}
```

### Failed Request (Permission Denied)

```json
{"timestamp":"2025-01-15T14:34:00+00:00","method":"/keel.v1.NodeService/Reboot","status":"PERMISSION_DENIED","duration_ms":0}
```

## Monitored Operations

All 22 gRPC API methods are audited automatically:

| Category | Methods |
|----------|---------|
| **System** | `GetStatus`, `Reboot`, `GetHealth` |
| **Updates** | `InstallUpdate`, `ScheduleUpdate`, `GetUpdateSchedule`, `CancelScheduledUpdate` |
| **Rollback** | `TriggerRollback`, `GetRollbackHistory` |
| **Certificates** | `InitBootstrap`, `RotateCertificate` |
| **Kubernetes** | `BootstrapKubernetes`, `GetBootstrapStatus` |
| **Network** | `ConfigureNetwork`, `GetNetworkConfig`, `GetNetworkStatus` |
| **Diagnostics** | `EnableDebugMode`, `GetDebugStatus`, `CollectCrashDump`, `StreamLogs`, `CreateSystemSnapshot`, `EnableRecoveryMode` |

## Integration with Monitoring

### OpenTelemetry

When the `OTLP_ENDPOINT` environment variable is set, audit events are exported alongside distributed traces. This allows correlation of audit events with system-wide traces in tools like Grafana Tempo, Jaeger, or Datadog.

### Log Aggregation

For centralized log management, configure your log collector (e.g. Fluentd, Vector, Filebeat) to tail `/var/lib/keel/audit/audit.log`. Since each line is valid JSON, no additional parsing is required.

Example Fluentd configuration:

```xml
<source>
  @type tail
  path /var/lib/keel/audit/audit.log
  pos_file /var/lib/keel/audit/audit.log.pos
  tag keel.audit
  <parse>
    @type json
  </parse>
</source>
```

## Security Considerations

- **Always-on**: Audit logging cannot be disabled, ensuring a complete record of all operations
- **Append-only**: The log file is opened in append mode to prevent overwriting
- **Durable writes**: Each entry is flushed to disk immediately after writing
- **Graceful degradation**: If the audit log file cannot be written (e.g. disk full), events are still emitted via `tracing` structured logging
- **Automatic recovery**: If the log file is removed or becomes inaccessible, the agent automatically reopens it on the next write
- **mTLS integration**: When mTLS is enabled, only authenticated clients can make API calls that generate audit entries

## Troubleshooting

### Audit log file not growing

If the audit log file at `/var/lib/keel/audit/audit.log` is not being written:

1. Verify the agent is running:
   ```bash
   osctl --endpoint http://<NODE_IP>:50051 status
   ```

2. Check for disk space issues via a crash dump:
   ```bash
   osctl --endpoint http://<NODE_IP>:50051 diag crash-dump
   ```

3. Review agent logs for audit-related warnings. Look for structured log entries containing `"Failed to write audit entry"` or `"Failed to open audit log file"`.

### Missing entries

Audit entries are recorded after the gRPC handler completes. If the agent process is terminated abruptly (e.g. power loss), the final in-flight request may not be logged.

## See Also

- [Diagnostics Guide](diagnostics.md) — Debug mode, crash dumps, and system snapshots
- [Diagnostics API Reference](../reference/diagnostics-api.md) — Detailed RPC and message definitions
- [osctl CLI Reference](../reference/osctl.md) — Complete CLI reference
- [KeelOS Architecture](../architecture.md) — System architecture overview
