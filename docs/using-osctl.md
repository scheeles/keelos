# Using osctl

`osctl` is the command-line tool for managing KeelOS nodes remotely. Since KeelOS has no SSH or shell access, `osctl` is the primary interface for administration.

## Installation

### Download Pre-built Binary

Download the latest `osctl` binary for your platform from [GitHub Releases](https://github.com/scheeles/keelos/releases).

| Platform | File |
|----------|------|
| macOS (Apple Silicon) | `osctl-Darwin_arm64.tar.gz` |
| macOS (Intel) | `osctl-Darwin_x86.tar.gz` |
| Linux (x86_64) | `osctl-Linux_x86.tar.gz` |
| Linux (ARM64) | `osctl-Linux_arm64.tar.gz` |
| Windows | `osctl-Windows_x86.zip` |

```bash
# Example for macOS ARM64
tar -xzf osctl-Darwin_arm64.tar.gz
sudo mv osctl /usr/local/bin/
```

### Build from Source

```bash
git clone https://github.com/scheeles/keelos.git
cd keelos
cargo build --release --package osctl
# Binary is at target/release/osctl
```

---

## Connecting to a Node

Every command requires a target node endpoint:

```bash
osctl --endpoint http://<NODE_IP>:50051 <command>
```

When testing locally with QEMU (using `run-qemu.sh`), the agent is forwarded to `localhost:50052`:

```bash
osctl --endpoint http://127.0.0.1:50052 <command>
```

The default endpoint is `http://[::1]:50051` (IPv6 loopback).

## Commands

### Check Node Status

```bash
osctl --endpoint http://127.0.0.1:50052 status
```

Returns system information including:
*   OS version
*   Uptime
*   Active partition (A or B)
*   Kubelet status

### Install an Update

```bash
osctl --endpoint http://127.0.0.1:50052 update --source https://example.com/keelos-v0.2.0.squashfs
```

This downloads the new OS image and writes it to the inactive partition. After installation, reboot to activate.

### Reboot the Node

```bash
osctl --endpoint http://127.0.0.1:50052 reboot
```

Initiates a graceful shutdown of all services and reboots into the newly installed partition.

### Join a Kubernetes Cluster

```bash
osctl --endpoint http://127.0.0.1:50052 bootstrap \
  --api-server https://k8s.example.com:6443 \
  --token abcdef.0123456789abcdef \
  --ca-cert ca.crt
```

Configures kubelet TLS bootstrapping so the node can join a Kubernetes cluster. See the [Kubernetes Bootstrap Guide](docs/guides/kubernetes-bootstrap.md) for detailed instructions.

### Check Bootstrap Status

```bash
osctl --endpoint http://127.0.0.1:50052 bootstrap-status
```

Shows whether the node is bootstrapped and the connection details.

### Network Configuration

```bash
# View network status
osctl --endpoint http://127.0.0.1:50052 network status

# Set static IP
osctl --endpoint http://127.0.0.1:50052 network config set \
  --interface eth0 --ip 10.0.0.5/24 --gateway 10.0.0.1

# Set DNS servers
osctl --endpoint http://127.0.0.1:50052 network dns set \
  --nameserver 8.8.8.8 --nameserver 1.1.1.1
```

### Diagnostics & Debugging

```bash
# Enable time-limited debug mode (15 min default)
osctl --endpoint http://127.0.0.1:50052 diag debug --reason "Investigating issue"

# Check debug mode status
osctl --endpoint http://127.0.0.1:50052 diag debug-status

# Collect crash dump (kernel + userspace)
osctl --endpoint http://127.0.0.1:50052 diag crash-dump

# Stream logs filtered by level
osctl --endpoint http://127.0.0.1:50052 diag logs --level error

# Create system snapshot
osctl --endpoint http://127.0.0.1:50052 diag snapshot --label "pre-upgrade"

# Enable recovery mode
osctl --endpoint http://127.0.0.1:50052 diag recovery --reason "Emergency repair"
```

For detailed diagnostics workflows, see the [Diagnostics Guide](guides/diagnostics.md).

## Authentication

`osctl` supports mutual TLS (mTLS) for secure communication with `keel-agent`. Certificates are managed automatically via a local cert store.

### Enable mTLS

```bash
# Generate a 24-hour bootstrap certificate
osctl init bootstrap --node <NODE_IP>
```

After this, all subsequent `osctl` commands automatically use mTLS. The certificates are stored locally and `osctl` will select the best available certificate (preferring operational over bootstrap) on each connection.

## Authorization (RBAC)

When mTLS is enabled, `keel-agent` enforces role-based access control based on the client certificate's Organization (O) field:

| Role | Certificate Organization (O) | Example Commands |
|------|-------------------------------|-----------------|
| **Admin** | `system:masters` or `keel:admin` | `reboot`, `rollback trigger`, `diag debug` |
| **Operator** | `keel:operator` | `update`, `diag snapshot`, `diag crash-dump` |
| **Viewer** | `keel:viewer` | `status`, `health`, `diag debug-status` |

Without mTLS (development mode), all requests are allowed regardless of role.

For detailed RBAC documentation, see the [RBAC Guide](guides/rbac.md).

For full CLI reference, see [osctl Reference](docs/reference/osctl.md).
