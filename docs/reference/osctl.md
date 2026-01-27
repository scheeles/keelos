# osctl CLI Reference

`osctl` is the command-line interface for managing KeelOS nodes. It interacts with the `keel-agent` via gRPC.

## Global Flags

| Flag | Description | Default |
| :--- | :--- | :--- |
| `--addr <address>` | Address of the target node (host:port). | `127.0.0.1:50051` |
| `--cert <path>` | Path to client certificate (mTLS). | *(Optional)* |
| `--key <path>` | Path to client key (mTLS). | *(Optional)* |
| `--ca <path>` | Path to Cluster CA certificate. | *(Optional)* |
| `--insecure` | Skip TLS verification (Dev/Test only). | `false` |

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
*   Individual check results

### `install`
Installs a new OS image to the inactive partition.
```bash
osctl install --image <url> [--verify-signature]
```
*   `--image`: URL or OCI reference to the SquashFS image.
*   `--verify-signature`: Enforce signature verification before installing.

### `reboot`
Reboots the node.
```bash
osctl reboot [--reason "Reason for reboot"]
```

### `schedule update`
Schedules an update for a future time or specific maintenance window.
```bash
osctl schedule update \
  --source <url> \
  --at "2023-12-01T04:00:00Z" \
  --enable-auto-rollback
```

### `rollback`
Manages rollback operations.
```bash
# View History
osctl rollback history

# Trigger Manual Rollback
osctl rollback trigger --reason "Emergency"
```

### `logs`
Streams logs from system components.
```bash
osctl logs --component <kubelet|containerd|agent> [--follow]
```
