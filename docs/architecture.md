# KeelOS Architecture

This document provides a high-level overview of the KeelOS architecture.

## Design Principles

1.  **Immutable**: The OS runs from a read-only SquashFS image. No package manager. No runtime modifications.
2.  **API-Driven**: All management happens via gRPC, not SSH.
3.  **Minimal Attack Surface**: No shell, no interpreters (Python/Perl), no unnecessary services.
4.  **Atomic Updates**: A/B partition scheme ensures safe, rollback-capable updates.

## Boot Sequence

```mermaid
sequenceDiagram
    participant BIOS/UEFI
    participant Kernel
    participant keel-init
    participant containerd
    participant kubelet
    participant keel-agent

    BIOS/UEFI->>Kernel: Load bzImage + initramfs
    Kernel->>keel-init: Execute /init (PID 1)
    keel-init->>keel-init: Mount /proc, /sys, /dev
    keel-init->>keel-init: Mount persistent storage
    keel-init->>containerd: Start containerd
    keel-init->>kubelet: Start kubelet
    keel-init->>keel-agent: Start gRPC agent
    keel-agent-->>keel-agent: Listen on :50051
```

## Component Responsibilities

| Component       | Role                                      |
|-----------------|-------------------------------------------|
| `keel-init`    | PID 1. Mounts filesystems, supervises processes, reaps zombies. |
| `keel-agent`   | gRPC server. Handles updates, reboots, configuration. |
| `osctl`         | CLI client for `keel-agent`. |
| `containerd`    | Container runtime (stock, unmodified). |
| `kubelet`       | Kubernetes node agent (stock, unmodified). |

## Partition Layout

KeelOS uses a GPT partition table with the following layout:

| Partition | Label         | Purpose                        |
|-----------|---------------|--------------------------------|
| 1         | `ESP`         | EFI System Partition (unused in direct kernel boot) |
| 2         | `MATIC_ROOT_A`| Primary OS image (SquashFS)    |
| 3         | `MATIC_ROOT_B`| Secondary OS image (for updates) |
| 4         | `MATIC_STATE` | Persistent data (overlay, etcd, logs) |

## Update Mechanism

### Full Image Update

1.  `osctl install` downloads a new SquashFS image.
2.  The image is written to the *inactive* partition (e.g., if booted from A, write to B).
3.  Boot flags are updated to boot from the new partition.
4.  On `osctl reboot`, the system boots into the new version.
5.  If the new version fails to boot, a watchdog triggers a rollback to the previous partition.

### Delta Update (Bandwidth-Efficient)

Delta updates use binary diff (bsdiff) to transfer only the differences between OS versions:

```mermaid
sequenceDiagram
    participant Client as osctl
    participant Agent as keel-agent
    participant Disk as Disk
    participant Server as Update Server
    
    Client->>Agent: InstallUpdate (is_delta=true)
    Agent->>Server: Download delta file
    Server-->>Agent: delta.bin (small)
    Agent->>Disk: Read active partition
    Disk-->>Agent: old_image
    Agent->>Agent: Apply bspatch(old_image, delta)
    Agent->>Disk: Write patched image to inactive partition
    Agent->>Agent: Verify SHA256
    Agent->>Disk: Switch boot flags
    Agent-->>Client: Success (bytes_saved reported)
```

**Fallback Strategy**: If delta application fails (corrupted delta, incompatible base, etc.), the agent automatically falls back to downloading the full image from `full_image_url`.

**Bandwidth Savings**: Typically 60-90% reduction in download size for minor version updates.

## Security Model

*   **mTLS**: All gRPC communication is secured with mutual TLS.
*   **Kernel Lockdown**: (Planned) Prevents runtime modification of kernel memory.
*   **Read-Only Root**: No writable paths on the OS image.
*   **No Passwords**: No user accounts. No `/etc/passwd`. No login.
