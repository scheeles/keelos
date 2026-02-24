# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

### Added
- **Kubelet TLS Bootstrap**: KeelOS nodes can now join Kubernetes clusters via `osctl bootstrap`. Supports bootstrap token and kubeconfig authentication methods.
- **Persistent Storage**: `keel-init` mounts a data disk and bind-mounts `/var/lib/containerd`, `/var/lib/kubelet`, and `/var/lib/keel` for persistent container and kubelet state.
- **Cgroup v2**: `keel-init` sets up cgroup v2 with `cpu`, `memory`, `io`, `pids`, and `cpuset` controllers for container resource management.
- **Kubelet Lifecycle Management**: `keel-init` supervision loop manages kubelet bootstrap → permanent kubeconfig transition and restart signals.
- **Pre-loaded Container Images**: `keel-init` imports pre-loaded images (e.g., pause container) into containerd on startup.
- **gRPC Keepalive**: `keel-agent` server and `osctl` client include HTTP/2 keepalive and timeouts for connection reliability.
- **Bootstrap Status**: `osctl bootstrap-status` command to check Kubernetes join state.
- **Network Management**: `osctl network` commands for configuring static IP, DHCP, IPv6, and DNS.
- **CI**: `test-bootstrap-kind` integration test validates end-to-end kubelet TLS bootstrapping using a Kind cluster.

## [v0.1.0] - 2026-01-27

### Added
- **Immutable OS Architecture**: Read-only SquashFS root filesystem.
- **gRPC Management API**: `keel-agent` for secure, API-driven management.
- **Update System**:
    - Atomic A/B partition updates.
    - **Delta Updates**: Binary diff updates using `bsdiff` for bandwidth efficiency.
    - Automatic rollback on boot failure.
- **CLI**: `osctl` command-line tool for managing the OS.
- **Documentation**: Comprehensive guides for architecture, lifecycle management, and installation.
- **License**: Apache License 2.0.

### Changed
- Renamed project from `maticos` to `keelos`.
