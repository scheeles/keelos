# keel-init

The PID 1 init process for KeelOS.

**Status**: Alpha
**Language**: Rust

## Responsibilities

`keel-init` is the first process launched by the kernel. Unlike `systemd` or `sysvinit`, it is not a general-purpose service manager. Its scope is strictly bounded:

1.  **Early Boot**:
    *   Mounts pseudo-filesystems (`/proc`, `/sys`, `/dev`, `/run`, `/sys/fs/cgroup`).
    *   Populates `/dev` using `devtmpfs`.
2.  **Storage Setup**:
    *   Scans block devices for the `MATIC_STATE` partition.
    *   Mounts the persistent overlay filesystem.
3.  **Networking**:
    *   Configures the loopback interface (`lo`).
    *   (Future) Sets up minimal host networking (DHCP or static).
4.  **Supervisor**:
    *   Starts and supervises `containerd`.
    *   Starts and supervises `kubelet`.
    *   Starts and supervises `keel-agent`.
5.  **Reaper**:
    *   Reaps orphaned zombie processes to prevent resource exhaustion.
6.  **Shutdown**:
    *   Handles `SIGTERM`/`SIGINT` to gracefully shut down services and unmount filesystems.

## Configuration

`keel-init` is largely zero-config, relying on convention (e.g., standard mount paths) rather than configuration files. However, it may read kernel command-line arguments (e.g., `matic.debug`) to toggle verbose logging.

## Development

To build:
```bash
cargo build --package keel-init --target x86_64-unknown-linux-musl
```
