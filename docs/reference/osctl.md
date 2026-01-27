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

### `cert`
Certificate management commands.

#### `cert status`
Display certificate status and expiry information.
```bash
osctl cert status
```
**Output:**
- Common Name
- Validity period (not before / not after)
- Days until expiry
- Warning if expiring soon (< 30 days)

**Example:**
```
🔐 Certificate Status

  Common Name: keel-agent
  Not Before:  2026-01-27T20:14:00Z
  Not After:   2026-04-27T20:14:00Z
  Days Until Expiry: 89 days
  ✅ Certificate is valid
```

#### `cert renew`
Manually trigger certificate rotation.
```bash
osctl cert renew
```

**Use Cases:**
- Security incident response
- Testing rotation mechanism
- Proactive renewal before long maintenance windows

**Example:**
```
✅ Certificate renewed successfully
   New expiry: 2026-04-27T20:14:00Z
```

#### `cert get-ca`
Retrieve the CA certificate for client enrollment.
```bash
# Print to stdout
osctl cert get-ca

# Save to file
osctl cert get-ca --output ca.pem
```

**Options:**
- `--output <file>`: Save CA certificate to specified file

**Example:**
```bash
$ osctl cert get-ca --output ca.pem
✅ CA certificate saved to: ca.pem
```

**See Also:** [mTLS Certificate Rotation Guide](../security/mtls-certificate-rotation.md)

### `logs`
Streams logs from system components.
```bash
osctl logs --component <kubelet|containerd|agent> [--follow]
```
