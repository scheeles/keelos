# osctl

The Command Line Interface for administering KeelOS nodes.

**Status**: Alpha
**Language**: Rust

## Overview

`osctl` is a remote client that talks to `keel-agent` via gRPC. It mimics a standard CLI tool but executes operations remotely.

## Usage

```bash
# Get node status
osctl --addr <NODE_IP>:50051 status

# Install a new OS version
osctl --addr <NODE_IP>:50051 install --image <OCI_IMAGE_URL> --version 1.0.1

# Reboot the node
osctl --addr <NODE_IP>:50051 reboot

# Stream logs
osctl --addr <NODE_IP>:50051 logs --component kubelet
```

## Configuration

Credentials and default endpoints can be stored in `~/.config/osctl.yaml` (Planned).

## Build

```bash
cargo build --package osctl
```
