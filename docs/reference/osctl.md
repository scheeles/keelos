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
