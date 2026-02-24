# Diagnostics & Debugging API Reference

This document describes the Diagnostics & Debugging API for KeelOS nodes.

## Overview

The Diagnostics API provides seven RPC methods for remote troubleshooting of KeelOS nodes:

- **EnableDebugMode**: Activate time-limited debug session with enhanced logging
- **GetDebugStatus**: Query current debug session status
- **CollectCrashDump**: Gather kernel and userspace diagnostic data
- **AnalyzeCrashDump**: Scan a collected crash dump for known failure patterns
- **StreamLogs**: Stream system logs with level and component filtering
- **CreateSystemSnapshot**: Create a point-in-time system state capture
- **EnableRecoveryMode**: Activate emergency recovery mode

All diagnostic sessions are **time-limited** (max 3600 seconds) and **audit-logged**.

## RPC Methods

### EnableDebugMode

Enables a time-limited debug session with enhanced logging. Only one session can be active at a time.

**Request**: `EnableDebugModeRequest`
```protobuf
message EnableDebugModeRequest {
  uint32 duration_secs = 1;  // Duration in seconds (max 3600)
  string reason = 2;          // Audit log reason
}
```

**Response**: `EnableDebugModeResponse`
```protobuf
message EnableDebugModeResponse {
  bool success = 1;
  string message = 2;
  string session_id = 3;   // Unique session identifier (UUID)
  string expires_at = 4;   // Expiry timestamp (RFC3339)
}
```

**Duration clamping:**
- `0` → default 900 seconds (15 minutes)
- `> 3600` → clamped to 3600 seconds (1 hour)

**Example (osctl)**:
```bash
osctl diag debug --duration 600 --reason "Investigating OOM kills"
```

**Example Output**:
```
🔧 Enabling debug mode...
✅ Debug mode enabled
  Session ID: a1b2c3d4-e5f6-7890-abcd-ef1234567890
  Expires at: 2025-01-15T14:30:00+00:00
```

**Error (duplicate session)**:
```
🔧 Enabling debug mode...
❌ Debug mode already active (session: a1b2c3d4, expires: 2025-01-15T14:30:00+00:00)
```

---

### GetDebugStatus

Returns the current debug session status.

**Request**: `GetDebugStatusRequest`
```protobuf
message GetDebugStatusRequest {}
```

**Response**: `GetDebugStatusResponse`
```protobuf
message GetDebugStatusResponse {
  bool enabled = 1;           // Whether debug mode is active
  string session_id = 2;      // Current session ID (empty if inactive)
  string expires_at = 3;      // Expiry timestamp (empty if inactive)
  string reason = 4;          // Reason provided when enabled
  uint32 remaining_secs = 5;  // Seconds until expiry
}
```

**Example (osctl)**:
```bash
osctl diag debug-status
```

**Example Output (active)**:
```
🔧 Debug Mode: ACTIVE
  Session ID: a1b2c3d4-e5f6-7890-abcd-ef1234567890
  Reason: Investigating OOM kills
  Expires at: 2025-01-15T14:30:00+00:00
  Remaining: 542s
```

**Example Output (inactive)**:
```
🔧 Debug Mode: INACTIVE
```

---

### CollectCrashDump

Collects kernel and/or userspace diagnostic data and saves it to disk.

**Request**: `CollectCrashDumpRequest`
```protobuf
message CollectCrashDumpRequest {
  bool include_kernel = 1;     // Include kernel data (dmesg)
  bool include_userspace = 2;  // Include userspace process info
}
```

**Response**: `CollectCrashDumpResponse`
```protobuf
message CollectCrashDumpResponse {
  bool success = 1;
  string message = 2;
  string dump_path = 3;         // On-node path to the dump file
  uint64 dump_size_bytes = 4;   // Size in bytes
  string created_at = 5;        // Creation timestamp (RFC3339)
}
```

**Dump contents:**
- **Kernel data** (`include_kernel`): `dmesg --time-format=iso` output
- **Userspace data** (`include_userspace`): `ps aux` process list and `/proc/meminfo`

**Storage location**: `/var/lib/keel/crash-dumps/crash-<YYYYMMDD-HHMMSS>.txt`

**Example (osctl)**:
```bash
osctl diag crash-dump
```

**Example Output**:
```
📦 Collecting crash dump...
✅ Crash dump collected successfully
  Path: /var/lib/keel/crash-dumps/crash-20250115-143000.txt
  Size: 245.67 KB
  Created: 2025-01-15T14:30:00+00:00
```

---

### AnalyzeCrashDump

Scans a previously collected crash dump file for known failure patterns and returns structured findings.

**Request**: `AnalyzeCrashDumpRequest`
```protobuf
message AnalyzeCrashDumpRequest {
  string dump_path = 1;  // On-node path to the crash dump file
}
```

**Response**: `AnalyzeCrashDumpResponse`
```protobuf
message CrashDumpFinding {
  string severity = 1;      // "critical", "error", "warning", "info"
  string finding_type = 2;  // "oom_kill", "kernel_panic", "segfault", "io_error", "stack_trace"
  string message = 3;       // Matching log line (human-readable)
}

message AnalyzeCrashDumpResponse {
  bool success = 1;
  string message = 2;
  string severity = 3;                     // Overall severity
  repeated CrashDumpFinding findings = 4;  // Individual findings
  string summary = 5;                      // Human-readable summary
}
```

**Detected patterns:**

| Finding Type | Severity | Trigger |
|---|---|---|
| `oom_kill` | critical | "out of memory", "oom_kill", "killed process" |
| `kernel_panic` | critical | "kernel panic", "bug:", "oops:" |
| `segfault` | error | "segfault", "general protection fault" |
| `io_error` | error | "i/o error", "ext4-fs error", "buffer i/o error" |
| `stack_trace` | warning | "call trace:", "rip:" |

**Overall severity precedence**: `critical > error > warning > clean` (when no findings)

**Example (osctl)**:
```bash
osctl diag analyze-dump --path /var/lib/keel/crash-dumps/crash-20250115-143000.txt
```

**Example Output (issues found)**:
```
🔍 Analyzing crash dump...
✅ Crash dump analyzed successfully
  Severity: critical
  Summary: Found 2 issue(s): kernel_panic, oom_kill

  Findings:
    [critical/kernel_panic] Kernel panic - not syncing: VFS unable to mount root fs
    [critical/oom_kill] Out of memory: Killed process 1234 (kubelet)
```

**Example Output (clean)**:
```
🔍 Analyzing crash dump...
✅ Crash dump analyzed successfully
  Severity: clean
  Summary: No significant issues found in crash dump.
```

---

### StreamLogs

Streams system logs with optional filtering. Returns historical lines first, then continues streaming.

**Request**: `StreamLogsRequest`
```protobuf
message StreamLogsRequest {
  string level = 1;       // Filter: "debug", "info", "warn", "error" (empty = all)
  string component = 2;   // Filter: component name (empty = all)
  uint32 tail_lines = 3;  // Historical lines to include (default: 50)
}
```

**Response**: (Server-streaming) `LogEntry`
```protobuf
message LogEntry {
  string timestamp = 1;  // Log timestamp (RFC3339)
  string level = 2;      // Log level
  string component = 3;  // Source component
  string message = 4;    // Log message
}
```

**Log level mapping** (from dmesg facility.level):
| dmesg Level | Mapped Level |
|-------------|-------------|
| `emerg`, `alert`, `crit`, `err` | `error` |
| `warn`, `warning` | `warn` |
| `notice`, `info` | `info` |
| `debug` | `debug` |

**Example (osctl)**:
```bash
osctl diag logs --level error --component kernel --tail 100
```

**Example Output**:
```
📜 Streaming logs...

[2025-01-15T14:30:00Z] error [kernel] Out of memory: Killed process 1234 (java)
[2025-01-15T14:30:01Z] error [kernel] oom_reaper: reaped process 1234 (java)
```

---

### CreateSystemSnapshot

Creates a point-in-time capture of system state for offline analysis.

**Request**: `CreateSystemSnapshotRequest`
```protobuf
message CreateSystemSnapshotRequest {
  string label = 1;          // Human-readable label
  bool include_config = 2;   // Include system config files
  bool include_logs = 3;     // Include recent kernel logs
}
```

**Response**: `CreateSystemSnapshotResponse`
```protobuf
message CreateSystemSnapshotResponse {
  bool success = 1;
  string message = 2;
  string snapshot_id = 3;     // Unique identifier (UUID)
  string snapshot_path = 4;   // On-node path
  uint64 size_bytes = 5;      // Size in bytes
  string created_at = 6;      // Creation timestamp (RFC3339)
}
```

**Snapshot contents:**
- **Configuration** (`include_config`): hostname, `/etc/os-release`, `/etc/keel/node.yaml`
- **Logs** (`include_logs`): last 200 lines of `dmesg --time-format=iso`

**Storage location**: `/var/lib/keel/snapshots/snapshot-<YYYYMMDD-HHMMSS>.txt`

**Example (osctl)**:
```bash
osctl diag snapshot --label "pre-upgrade-v1.2.0"
```

**Example Output**:
```
📸 Creating system snapshot...
✅ System snapshot created successfully
  Snapshot ID: f7e8d9c0-b1a2-3456-7890-abcdef012345
  Path: /var/lib/keel/snapshots/snapshot-20250115-143000.txt
  Size: 12.34 KB
  Created: 2025-01-15T14:30:00+00:00
```

---

### EnableRecoveryMode

Enables time-limited emergency recovery mode. Only one recovery session can be active at a time.

**Request**: `EnableRecoveryModeRequest`
```protobuf
message EnableRecoveryModeRequest {
  string reason = 1;         // Audit log reason
  uint32 duration_secs = 2;  // Duration in seconds (max 3600)
}
```

**Response**: `EnableRecoveryModeResponse`
```protobuf
message EnableRecoveryModeResponse {
  bool success = 1;
  string message = 2;
  string expires_at = 3;  // Expiry timestamp (RFC3339)
}
```

**Duration clamping** (same as debug mode):
- `0` → default 900 seconds (15 minutes)
- `> 3600` → clamped to 3600 seconds (1 hour)

**Example (osctl)**:
```bash
osctl diag recovery --duration 1800 --reason "Emergency kernel investigation"
```

**Example Output**:
```
🚨 Enabling recovery mode...
✅ Recovery mode enabled (reason: Emergency kernel investigation)
  Expires at: 2025-01-15T15:00:00+00:00
```

## Storage Locations

| Data | Path |
|------|------|
| Crash dumps | `/var/lib/keel/crash-dumps/crash-<timestamp>.txt` |
| Snapshots | `/var/lib/keel/snapshots/snapshot-<timestamp>.txt` |

Both directories are on persistent storage and survive reboots.

## Security Considerations

- All diagnostic sessions are **time-limited** (max 1 hour) and auto-expire
- All enable operations are **audit-logged** with caller-provided reason
- **No duplicate sessions** — attempting to enable while active returns an error
- Communication is protected by **mTLS** when configured
- Crash dumps and snapshots are written to node-local persistent storage

## See Also

- [Diagnostics & Debugging Guide](../guides/diagnostics.md) — How-to guide with troubleshooting workflows
- [osctl CLI Reference](./osctl.md) — Complete CLI reference
- [API Reference](./api.md) — Core API reference
