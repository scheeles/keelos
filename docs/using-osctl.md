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

Every command requires a target node address:

```bash
osctl --addr <NODE_IP>:50051 <command>
```

When testing locally with QEMU (using `run-qemu.sh`), the agent is forwarded to `localhost:50052`:

```bash
osctl --addr 127.0.0.1:50052 <command>
```

## Commands

### Check Node Status

```bash
osctl --addr 127.0.0.1:50052 status
```

Returns system information including:
*   OS version
*   Uptime
*   Active partition (A or B)
*   Kubelet status

### Install an Update

```bash
osctl --addr 127.0.0.1:50052 install --image oci://registry.example.com/keelos:1.0.1
```

This downloads the new OS image and writes it to the inactive partition. After installation, reboot to activate.

### Reboot the Node

```bash
osctl --addr 127.0.0.1:50052 reboot
```

Initiates a graceful shutdown of all services and reboots into the newly installed partition.

### Stream Logs

```bash
osctl --addr 127.0.0.1:50052 logs --component kubelet
osctl --addr 127.0.0.1:50052 logs --component containerd
osctl --addr 127.0.0.1:50052 logs --component agent
```

Streams real-time logs from the specified component.

### Network Configuration (Planned)

```bash
osctl --addr 127.0.0.1:50052 network set --ip 10.0.0.5/24 --gateway 10.0.0.1
```

## Authentication

> [!NOTE]
> mTLS authentication is planned but not yet implemented in the alpha release.

In production, `osctl` will require client certificates to authenticate with the agent.
