# Diagnostics & Debugging Guide

This guide explains how to use KeelOS diagnostics tools for troubleshooting production nodes. Since KeelOS has no SSH or shell access, all diagnostics are performed remotely via the `osctl diag` command.

## Overview

KeelOS provides six diagnostic capabilities:

| Tool | Purpose |
|------|---------|
| **Debug Mode** | Time-limited session with enhanced logging |
| **Debug Status** | Check if debug mode is active |
| **Crash Dump** | Collect kernel and userspace diagnostic data |
| **Log Streaming** | Stream and filter system logs in real time |
| **Snapshot** | Create a point-in-time system state backup |
| **Recovery Mode** | Enable emergency recovery for critical repairs |

All diagnostic operations are **audit-logged** and **time-limited** (max 1 hour) to prevent accidental exposure.

## Prerequisites

- `osctl` installed and configured ([Installation](../using-osctl.md))
- Network access to the target node on port `50051`
- mTLS certificates configured (recommended for production)

## Debug Mode

Debug mode enables enhanced logging and diagnostics for a limited time window. Sessions auto-expire to prevent indefinite exposure.

### Enable Debug Mode

```bash
osctl --endpoint http://<NODE_IP>:50051 diag debug \
  --duration 600 \
  --reason "Investigating pod scheduling failures"
```

**Output:**
```
🔧 Enabling debug mode...
✅ Debug mode enabled
  Session ID: a1b2c3d4-e5f6-7890-abcd-ef1234567890
  Expires at: 2025-01-15T14:30:00+00:00
```

**Parameters:**

| Flag | Description | Default |
|------|-------------|---------|
| `--duration` | Session length in seconds (max 3600) | `900` (15 min) |
| `--reason` | Audit log reason | `"Manual debug via osctl"` |

> **Note:** Only one debug session can be active at a time. Attempting to enable a second session while one is active will fail with an error.

### Check Debug Status

```bash
osctl --endpoint http://<NODE_IP>:50051 diag debug-status
```

**When active:**
```
🔧 Debug Mode: ACTIVE
  Session ID: a1b2c3d4-e5f6-7890-abcd-ef1234567890
  Reason: Investigating pod scheduling failures
  Expires at: 2025-01-15T14:30:00+00:00
  Remaining: 542s
```

**When inactive:**
```
🔧 Debug Mode: INACTIVE
```

## Crash Dump Collection

Crash dumps gather kernel messages (dmesg) and userspace process information for offline analysis.

### Collect a Crash Dump

```bash
osctl --endpoint http://<NODE_IP>:50051 diag crash-dump
```

**Output:**
```
📦 Collecting crash dump...
✅ Crash dump collected successfully
  Path: /var/lib/keel/crash-dumps/crash-20250115-143000.txt
  Size: 245.67 KB
  Created: 2025-01-15T14:30:00+00:00
```

The dump includes:
- **Kernel messages** (dmesg with ISO timestamps)
- **Process list** (`ps aux`)
- **Memory information** (`/proc/meminfo`)

### Selective Collection

To collect only kernel data (without userspace process info):

```bash
osctl --endpoint http://<NODE_IP>:50051 diag crash-dump --no-userspace
```

To collect only userspace info (without kernel messages):

```bash
osctl --endpoint http://<NODE_IP>:50051 diag crash-dump --no-kernel
```

## Log Streaming

Stream system logs with optional filtering by level and component.

### Stream All Logs

```bash
osctl --endpoint http://<NODE_IP>:50051 diag logs
```

**Output:**
```
📜 Streaming logs...

[2025-01-15T14:30:00Z] info [kernel] eth0: link up
[2025-01-15T14:30:01Z] info [kernel] keel-agent started
[2025-01-15T14:30:02Z] warn [kernel] low memory threshold reached
```

### Filter by Log Level

Show only errors:

```bash
osctl --endpoint http://<NODE_IP>:50051 diag logs --level error
```

Available levels: `debug`, `info`, `warn`, `error`

### Filter by Component

Show only logs from a specific component:

```bash
osctl --endpoint http://<NODE_IP>:50051 diag logs --component kernel
```

### Control History

Include more historical lines before streaming:

```bash
osctl --endpoint http://<NODE_IP>:50051 diag logs --tail 100
```

### Combined Filters

```bash
osctl --endpoint http://<NODE_IP>:50051 diag logs \
  --level error \
  --component kernel \
  --tail 200
```

## System Snapshots

Create a point-in-time capture of system state for offline analysis or backup.

### Create a Snapshot

```bash
osctl --endpoint http://<NODE_IP>:50051 diag snapshot \
  --label "pre-upgrade-v1.2.0"
```

**Output:**
```
📸 Creating system snapshot...
✅ System snapshot created successfully
  Snapshot ID: f7e8d9c0-b1a2-3456-7890-abcdef012345
  Path: /var/lib/keel/snapshots/snapshot-20250115-143000.txt
  Size: 12.34 KB
  Created: 2025-01-15T14:30:00+00:00
```

The snapshot includes:
- **System Configuration** — hostname, `/etc/os-release`, KeelOS node config
- **Recent Kernel Logs** — last 200 lines of dmesg

### Snapshot Options

| Flag | Description | Default |
|------|-------------|---------|
| `--label` | Human-readable label | `"manual snapshot"` |
| `--config` | Include system configuration | `true` |
| `--logs` | Include recent kernel logs | `true` |

### Typical Use Cases

**Before an upgrade:**
```bash
osctl diag snapshot --label "pre-upgrade-v1.2.0"
osctl update --source https://releases.keelos.dev/v1.2.0.squashfs
osctl reboot
```

**After an incident:**
```bash
osctl diag snapshot --label "post-incident-2025-01-15"
osctl diag crash-dump
```

## Recovery Mode

Recovery mode enables emergency access for critical repairs. Like debug mode, it is time-limited and audit-logged.

### Enable Recovery Mode

```bash
osctl --endpoint http://<NODE_IP>:50051 diag recovery \
  --duration 1800 \
  --reason "Emergency kernel module investigation"
```

**Output:**
```
🚨 Enabling recovery mode...
✅ Recovery mode enabled (reason: Emergency kernel module investigation)
  Expires at: 2025-01-15T15:00:00+00:00
```

**Parameters:**

| Flag | Description | Default |
|------|-------------|---------|
| `--duration` | Recovery window in seconds (max 3600) | `900` (15 min) |
| `--reason` | Audit log reason | `"Manual recovery via osctl"` |

> **⚠️ Warning:** Recovery mode provides elevated access. Always use the shortest duration necessary and document the reason.

## Common Troubleshooting Workflows

### Node Not Joining Cluster

1. Check node status:
   ```bash
   osctl status
   ```

2. Enable debug mode for detailed logging:
   ```bash
   osctl diag debug --duration 900 --reason "Node not joining cluster"
   ```

3. Stream error logs:
   ```bash
   osctl diag logs --level error
   ```

4. Collect crash dump for analysis:
   ```bash
   osctl diag crash-dump
   ```

### Node Health Degraded

1. Check health status:
   ```bash
   osctl health
   ```

2. Create a snapshot for baseline:
   ```bash
   osctl diag snapshot --label "health-investigation"
   ```

3. Stream warnings and errors:
   ```bash
   osctl diag logs --level warn --tail 200
   ```

### Pre-Upgrade Checklist

1. Verify current health:
   ```bash
   osctl health
   ```

2. Create pre-upgrade snapshot:
   ```bash
   osctl diag snapshot --label "pre-upgrade-$(date +%Y%m%d)"
   ```

3. Proceed with upgrade:
   ```bash
   osctl update --source <url>
   osctl reboot
   ```

4. If issues occur, collect diagnostics:
   ```bash
   osctl diag crash-dump
   osctl diag logs --level error --tail 500
   ```

## Security Considerations

- **Time-limited sessions**: Debug and recovery modes auto-expire (max 1 hour)
- **Audit logging**: All enable/disable operations are logged with the caller's reason
- **No duplicate sessions**: Only one debug/recovery session can be active at a time
- **mTLS**: All communication is encrypted and authenticated via mutual TLS
- **Crash dumps on disk**: Dumps are saved to `/var/lib/keel/crash-dumps/` on the node's persistent storage
- **Snapshots on disk**: Snapshots are saved to `/var/lib/keel/snapshots/` on the node's persistent storage

## See Also

- [Diagnostics API Reference](../reference/diagnostics-api.md) — Detailed RPC and message definitions
- [osctl CLI Reference](../reference/osctl.md) — Complete CLI reference
- [KeelOS Architecture](../architecture.md) — System architecture overview
