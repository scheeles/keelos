# osctl

The Command Line Interface for administering KeelOS nodes.

**Status**: Alpha
**Language**: Rust

## Overview

`osctl` is a remote client that talks to `keel-agent` via gRPC. It auto-loads mTLS certificates from a local cert store when available, falling back to plain HTTP otherwise.

## Usage

```bash
# Get node status
osctl --endpoint http://<NODE_IP>:50051 status

# Install a new OS version
osctl --endpoint http://<NODE_IP>:50051 update --source <IMAGE_URL>

# Reboot the node
osctl --endpoint http://<NODE_IP>:50051 reboot

# Join a Kubernetes cluster
osctl --endpoint http://<NODE_IP>:50051 bootstrap \
  --api-server https://<K8S_API>:6443 \
  --token <TOKEN> --ca-cert ca.crt

# Check bootstrap status
osctl --endpoint http://<NODE_IP>:50051 bootstrap-status

# Enable mTLS
osctl init bootstrap --node <NODE_IP>
```

## Configuration

Certificates are managed via a local cert store (`~/.config/osctl/`). Run `osctl init bootstrap --node <ip>` to generate and exchange bootstrap certificates.

## Build

```bash
cargo build --package osctl
```
