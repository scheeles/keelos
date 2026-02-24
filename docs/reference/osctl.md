# osctl CLI Reference

`osctl` is the command-line interface for managing KeelOS nodes. It communicates with `keel-agent` via gRPC and auto-loads mTLS certificates from a local cert store when available.

## Global Flags

| Flag | Description | Default |
| :--- | :--- | :--- |
| `--endpoint <url>` | gRPC endpoint of the target node. | `http://[::1]:50051` |

> [!TIP]
> Run `osctl init bootstrap --node <ip>` to enable mTLS. After that, `osctl` auto-loads certificates from the local cert store for all subsequent connections.

## Commands

### `status`
Retrieves the current status of the node.
```bash
osctl status
```
**Output:**
*   Hostname
*   Kernel/OS Version
*   Uptime
*   Active Partition

### `health`
Runs a health check on the node.
```bash
osctl health
```
**Output:**
*   Overall Status (`healthy`, `degraded`, `unhealthy`)
*   Individual check results with duration

### `update`
Installs a new OS image to the inactive partition.
```bash
osctl update --source <url> [--sha256 <hash>] [--delta] [--fallback] [--full-image-url <url>]
```
*   `--source`: URL of the SquashFS image (or delta file if `--delta` is set).
*   `--sha256`: Expected SHA256 checksum for verification.
*   `--delta`: Treat the source as a delta file.
*   `--fallback`: Fall back to full image download if delta fails.
*   `--full-image-url`: URL for the full image (used as fallback).

### `reboot`
Reboots the node.
```bash
osctl reboot [--reason "Reason for reboot"]
```

### `bootstrap`
Joins the node to a Kubernetes cluster via kubelet TLS bootstrapping.
```bash
osctl bootstrap \
  --api-server <url> \
  [--token <token> --ca-cert <path>] \
  [--kubeconfig <path>] \
  [--node-name <name>]
```
*   `--api-server`: Kubernetes API server endpoint (required).
*   `--token`: Bootstrap token (`<token-id>.<token-secret>`). Requires `--ca-cert`.
*   `--ca-cert`: Path to the cluster CA certificate file.
*   `--kubeconfig`: Path to a pre-generated kubeconfig file (alternative to token auth).
*   `--node-name`: Override the node name (default: hostname).

Either `--token` (with `--ca-cert`) or `--kubeconfig` must be provided.

### `bootstrap-status`
Shows the current Kubernetes bootstrap state.
```bash
osctl bootstrap-status
```
**Output:**
*   Whether the node is bootstrapped
*   API server endpoint
*   Node name
*   Kubeconfig path
*   Bootstrap timestamp

### `rollback`
Manages rollback operations.
```bash
# View History
osctl rollback history

# Trigger Manual Rollback
osctl rollback trigger [--reason "Emergency"]
```

### `init`
Certificate initialization commands.

```bash
# Generate a 24-hour bootstrap certificate for mTLS
osctl init bootstrap --node <ip>

# Initialize with Kubernetes-signed operational certificate (planned)
osctl init kubeconfig
```

### `network`
Network management commands.

```bash
# Show network status
osctl network status

# Configure static IP
osctl network config set --interface eth0 --ip 10.0.0.5/24 --gateway 10.0.0.1

# Configure DHCP
osctl network config set --interface eth0 --dhcp

# Configure IPv6
osctl network config set --interface eth0 --ipv6 fd00::1/64 --ipv6-gateway fd00::1

# Enable IPv6 SLAAC
osctl network config set --interface eth0 --ipv6-auto

# Show saved network config
osctl network config show

# Set DNS servers
osctl network dns set --nameserver 8.8.8.8 --nameserver 1.1.1.1
```

### `diag`
Diagnostics and debugging commands. All sessions are time-limited (max 1 hour) and audit-logged.

```bash
# Enable time-limited debug mode
osctl diag debug [--duration <secs>] [--reason "why"]

# Check debug mode status
osctl diag debug-status

# Collect crash dump (kernel + userspace)
osctl diag crash-dump [--kernel true] [--userspace true]

# Analyze a previously collected crash dump
osctl diag analyze-dump --path <dump-file-path>

# Stream logs with filters
osctl diag logs [--level <level>] [--component <name>] [--tail <n>]

# Create system snapshot
osctl diag snapshot [--label <text>] [--config true] [--logs true]

# Enable emergency recovery mode
osctl diag recovery [--duration <secs>] [--reason "why"]
```

#### `diag debug`
Enables a time-limited debug session with enhanced logging.
*   `--duration`: Session length in seconds (default: 900, max: 3600).
*   `--reason`: Audit log reason (default: `"Manual debug via osctl"`).

#### `diag debug-status`
Shows whether debug mode is currently active, including session ID, reason, and remaining time.

#### `diag crash-dump`
Collects kernel messages (dmesg) and userspace process information.
*   `--kernel`: Include kernel crash data (default: `true`).
*   `--userspace`: Include userspace process info (default: `true`).

Dumps are saved to `/var/lib/keel/crash-dumps/` on the target node.

#### `diag analyze-dump`
Analyzes a previously collected crash dump file for known failure patterns.
*   `--path`: Path to the crash dump file on the target node (required).

Reports an overall severity (`critical`, `error`, `warning`, `clean`) and individual findings with their type and matching log line. Detected patterns include OOM kills, kernel panics, segfaults, I/O errors, and stack traces.

#### `diag logs`
Streams system logs with optional filtering.
*   `--level`: Filter by level (`debug`, `info`, `warn`, `error`).
*   `--component`: Filter by component name (e.g., `kernel`).
*   `--tail`: Number of historical lines to include (default: 50).

#### `diag snapshot`
Creates a point-in-time system state capture.
*   `--label`: Human-readable label (default: `"manual snapshot"`).
*   `--config`: Include system configuration (default: `true`).
*   `--logs`: Include recent kernel logs (default: `true`).

Snapshots are saved to `/var/lib/keel/snapshots/` on the target node.

#### `diag recovery`
Enables time-limited emergency recovery mode.
*   `--duration`: Recovery window in seconds (default: 900, max: 3600).
*   `--reason`: Audit log reason (default: `"Manual recovery via osctl"`).

For detailed usage and troubleshooting workflows, see the [Diagnostics Guide](../guides/diagnostics.md).
