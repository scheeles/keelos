# KeelOS

> **A Next-Generation, "No-General-Purpose-OS" for Kubernetes Nodes.**

KeelOS is an immutable, API-driven Linux distribution designed exclusively for hosting Kubernetes workloads. It eliminates the traditional userspace (Shell, SSH, Systemd) in favor of a single-binary PID 1 and a gRPC-based management API.

## Philosophy

1.  **Immutable**: The OS is a read-only SquashFS image. Updates are atomic A/B partition swaps.
2.  **API-Driven**: No SSH. No Console. All management happens via authenticated gRPC calls.
3.  **Minimalist**: Under 100MB. Only the kernel, `keel-init`, `keel-agent`, `containerd`, and `kubelet`.
4.  **Secure**: mTLS everywhere. Kernel lockdown. No interpreters (Python/Perl).

## Directory Structure

*   **/kernel**: Minimalist Linux Kernel configuration and patches.
*   **/pkg**: Shared Go/Rust libraries for the OS components.
*   **/cmd**: Binaries (`keel-init`, `keel-agent`, `osctl`).
*   **/system**: Static manifests and bootstrap configuration.
*   **/tools**: Build systems and test harnesses.
*   **/docs**: Documentation and API specifications.
*   **/.ai-context**: Documentation for AI agents contributing to this repo.


## Quick Start

### Building

Build the OS image:
```bash
./tools/builder/build.sh
```

### Running

Run in QEMU for testing:
```bash
./tools/testing/run-qemu.sh
```

### Managing Updates

Check system health:
```bash
osctl health
```

Schedule an update with automatic rollback:
```bash
osctl schedule update \
  --source http://update-server/os-v2.0.squashfs \
  --enable-auto-rollback \
  --health-check-timeout 300
```

Manually trigger rollback if needed:
```bash
osctl rollback trigger --reason "Emergency recovery"
```

View rollback history:
```bash
osctl rollback history
```

Use delta updates to save bandwidth:
```bash
# Generate delta between versions (on build server)
./tools/builder/generate-delta.sh os-v1.0.squashfs os-v1.1.squashfs update.delta

# Apply delta update with automatic fallback
osctl update \
  --source http://update-server/update.delta \
  --delta \
  --fallback \
  --full-image-url http://update-server/os-v1.1.squashfs
```

### Certificate Management

KeelOS provides automatic certificate management with dual-CA support:

**Development (Bootstrap Certificates):**
```bash
# Generate 24-hour self-signed certificate
osctl init bootstrap
```

**Production (Kubernetes Operational Certificates):**
```bash
# Auto-initialized when agent runs in K8s cluster
# Auto-renews 30 days before expiry
# Monitored via OpenTelemetry metrics
```

See [Certificate Management Guide](docs/certificate-management.md) for details.

### Kubernetes Node Bootstrap

Add a KeelOS node to a K8s cluster:
```bash
# Create bootstrap token on control plane
kubectl create token node-bootstrapper --duration=24h --namespace=kube-system

# Extract CA certificate
kubectl config view --raw -o jsonpath='{.clusters[0].cluster.certificate-authority-data}' | base64 -d > ca.crt

osctl init kubernetes \
  --token <bootstrap-token> \
  --ca-cert ca.crt

# Verify node joined
kubectl get nodes
```

## Documentation

- **[Overview](docs/overview/what-is-keelos.md)**: Introduction, Philosophy, and Security.
- **[Getting Started](docs/getting-started/quickstart-qemu.md)**: Run KeelOS locally in QEMU.
- **[Core Concepts](docs/learn-more/architecture.md)**: Architecture, API-Management, and Immutability.
- **[Certificate Management](docs/certificate-management.md)**: Auto-renewal, dual-CA, and monitoring.
- **[Operational Guides](docs/operational-guides/lifecycle-management.md)**: Updates, Rollbacks, and Configuration.
- **[Platform Installation](docs/platform-installation/local-qemu.md)**: Installation guides for Local, Bare Metal, and Cloud.
- **[Reference](docs/reference/osctl.md)**: CLI and API Reference.

## Development

Run integration tests:
```bash
./tools/test-rollback-flow.sh
```

Build individual components:
```bash
# Build keel-agent
cargo build --package keel-agent

# Build osctl CLI
cargo build --package osctl
```

## License

This project is licensed under the Apache License 2.0.
