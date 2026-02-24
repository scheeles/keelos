# keel-init

The PID 1 init process for KeelOS.

**Status**: Alpha
**Language**: Rust

## Responsibilities

`keel-init` is the first process launched by the kernel. Unlike `systemd` or `sysvinit`, it is not a general-purpose service manager. Its scope is strictly bounded:

1.  **Early Boot**:
    *   Mounts pseudo-filesystems (`/proc`, `/sys`, `/dev`, `/run`, `/tmp`, `/sys/fs/cgroup`).
    *   Populates `/dev` using `devtmpfs`.
2.  **Persistent Storage**:
    *   Detects data disk candidates (`/dev/sda4`, `/dev/sda`).
    *   Formats with ext4 if unformatted, mounts to `/data`.
    *   Bind-mounts `/data/containerd` → `/var/lib/containerd`, `/data/kubelet` → `/var/lib/kubelet`, `/data/keel` → `/var/lib/keel`.
3.  **Cgroup v2 Setup**:
    *   Mounts cgroup2 filesystem at `/sys/fs/cgroup`.
    *   Enables controllers: `cpu`, `memory`, `io`, `pids`, `cpuset`.
4.  **Hostname**:
    *   Reads saved hostname from `/var/lib/keel/hostname` or generates a unique fallback.
5.  **Networking**:
    *   Configures the loopback interface (`lo`).
    *   Applies saved network configuration or falls back to DHCP.
6.  **Supervisor**:
    *   Starts and supervises `keel-agent`, `containerd`, and `kubelet`.
    *   Restarts `containerd` and `keel-agent` automatically on crash (with exponential backoff for the agent).
    *   Manages kubelet bootstrap lifecycle: detects bootstrap kubeconfig, handles restart signals from `keel-agent`, and switches kubelet from bootstrap to permanent kubeconfig after CSR approval.
    *   Imports pre-loaded container images into containerd on startup.
7.  **Reaper**:
    *   Reaps orphaned zombie processes to prevent resource exhaustion.
8.  **Shutdown**:
    *   Handles `SIGTERM`/`SIGINT` to gracefully shut down services and unmount filesystems.

## Kubelet Modes

`keel-init` manages three kubelet operating modes:

| Mode | Condition | Behavior |
|------|-----------|----------|
| **Standalone** | No kubeconfig files exist | Kubelet runs without cluster connection |
| **Bootstrap** | Bootstrap kubeconfig exists, no permanent kubeconfig | Kubelet uses `--bootstrap-kubeconfig` to submit CSR |
| **Cluster** | Permanent kubeconfig exists | Kubelet uses permanent credentials |

The supervision loop watches for the restart signal file (`/run/keel/restart-kubelet`) created by `keel-agent` during the `osctl bootstrap` flow, and also detects when the permanent kubeconfig appears after CSR approval.

## Configuration

`keel-init` is largely zero-config, relying on convention (e.g., standard mount paths) rather than configuration files. However, it may read kernel command-line arguments (e.g., `keel.test_mode`) to toggle test behavior.

## Development

To build:
```bash
cargo build --package keel-init --target x86_64-unknown-linux-musl
```
