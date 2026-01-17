# MaticOS

> **A Next-Generation, "No-General-Purpose-OS" for Kubernetes Nodes.**

MaticOS is an immutable, API-driven Linux distribution designed exclusively for hosting Kubernetes workloads. It eliminates the traditional userspace (Shell, SSH, Systemd) in favor of a single-binary PID 1 and a gRPC-based management API.

## Philosophy

1.  **Immutable**: The OS is a read-only SquashFS image. Updates are atomic A/B partition swaps.
2.  **API-Driven**: No SSH. No Console. All management happens via authenticated gRPC calls.
3.  **Minimalist**: Under 100MB. Only the kernel, `matic-init`, `matic-agent`, `containerd`, and `kubelet`.
4.  **Secure**: mTLS everywhere. Kernel lockdown. No interpreters (Python/Perl).

## Directory Structure

*   **/kernel**: Minimalist Linux Kernel configuration and patches.
*   **/pkg**: Shared Go/Rust libraries for the OS components.
*   **/cmd**: Binaries (`matic-init`, `matic-agent`, `osctl`).
*   **/system**: Static manifests and bootstrap configuration.
*   **/tools**: Build systems and test harnesses.
*   **/.ai-context**: Documentation for AI agents contributing to this repo.

## Getting Started

*(Coming Soon in Phase 1)*

To build the OS image:
```bash
./tools/builder/build.sh
```

To run in QEMU:
```bash
./tools/testing/run-qemu.sh
```
