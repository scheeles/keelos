# Using osctl

`osctl` is the command-line tool for managing MaticOS nodes remotely. Since MaticOS has no SSH or shell access, `osctl` is the primary interface for administration.

## Connecting to a Node

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
osctl --addr 127.0.0.1:50052 install --image oci://registry.example.com/maticos:1.0.1
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
