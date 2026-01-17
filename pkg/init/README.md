# pkg/init (Matic Init)

**Responsibility**: PID 1 logic.

This package contains the core logic for the custom init process. `matic-init` is responsible for the very first steps of userspace.

## Responsibilities

1.  **Mount API Filesystems**: `/proc`, `/sys`, `/dev`, `/run`.
2.  **Root Setup**: Switch root to the actual OS if inside initramfs (future), or set up OverlayFS.
3.  **Network Bootstrap**: Bring up `lo` (loopback).
4.  **Service Supervision**:
    *   Spawn `matic-agent` (PID 2) as a child.
    *   Monitor it and restart it if it crashes (up to a limit).
5.  **Reaper**: Continuously reap orphaned zombie processes (waitpid loop).

## Constraints

*   **No Panic**: If this binary crashes, the kernel crashes (Kernel Panic).
*   **Performance**: Must start almost instantly.
*   **Dependencies**: Keep minimal.
