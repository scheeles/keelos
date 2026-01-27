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

## Features

### ✅ Phase 1: Update Scheduling (Complete)
- Scheduled OS updates with maintenance windows
- Pre/post-update hooks for custom workflows  
- Persistent schedule tracking across reboots
- gRPC API for update management

### ✅ Phase 2: Automatic Rollback (Complete)
- **Health Check Framework**: Pluggable health checks (boot, service, network, API)
- **Automatic Rollback**: Failed health checks trigger automatic partition rollback
- **Boot Loop Protection**: Detects boot loops and prevents infinite rollback cycles
- **Manual Rollback**: Emergency rollback via CLI or API
- **Rollback History**: Audit trail of all rollback events

### 🚧 Phase 3: Delta Updates (Planned)
- Binary diff generation for efficient updates
- Reduced bandwidth usage
- Incremental update support

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

## Documentation

- [Getting Started Guide](docs/getting-started.md)
- [Architecture Overview](docs/architecture.md)
- [Health Check and Rollback API](docs/api/health-and-rollback.md)
- [Using osctl](docs/using-osctl.md)

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
