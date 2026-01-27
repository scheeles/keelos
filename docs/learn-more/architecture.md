# Architecture

KeelOS transforms the traditional Linux OS into a single-purpose Kubernetes hosting appliance. It discards the complexity of general-purpose distributions in favor of a minimalist, immutable, and API-driven design.

## System Overview

At a high level, the system consists of:
1.  **Linux Kernel**: A minimal kernel configuration.
2.  **keel-init (PID 1)**: The init system and supervisor.
3.  **keel-agent**: The API server for management.
4.  **Runtime Components**: `containerd` and `kubelet`.

There is no userspace shell, no SSH, and no Systemd.

## The Init Process (PID 1)

In KeelOS, `/init` is a custom Rust binary known as `keel-init`. It is statically linked and responsible for the entire lifecycle of the system.

### Responsibilities
*   **Early Boot**: Mounts pseudo-filesystems (`/proc`, `/sys`, `/dev`).
*   **Hardware Setup**: Loads necessary kernel modules and brings up the loopback interface.
*   **Partition Management**: Detects the persistent data partition. If it's missing (first boot), it formats the disk and creates the necessary layout.
*   **Supervision**: Starts and monitors `keel-agent`, `containerd`, and `kubelet`. If these critical services crash, they are restarted.
*   **Signal Handling**: Reaps zombie processes to prevent resource exhaustion.

## Partition Layout

KeelOS employs a dual-root partition scheme to enable atomic updates.

| Partition | Label | Filesystem | Role |
| :--- | :--- | :--- | :--- |
| **EFI** | `EFI-SYSTEM` | VFAT | UEFI bootloader (Limine/GRUB). |
| **Root A** | `KEELOS_ROOT_A` | SquashFS | Read-only OS image (Slot A). |
| **Root B** | `KEELOS_ROOT_B` | SquashFS | Read-only OS image (Slot B). |
| **Data** | `KEELOS_DATA` | Ext4 | Persistent data (`/var/lib/keel`). |

*   **Immutability**: The active root partition is mounted read-only. No system binaries can be modified.
*   **Persistence**: Kubernetes data (etcd, container logs, images) is stored on the Data partition. This is the *only* writable area of the disk.

## Component Interaction

```mermaid
graph TD
    User[Administrator] -->|gRPC / osctl| Agent[keel-agent]
    Agent -->|Updates| Partitions[Disk Partitions]
    Agent -->|Reboot| Init[keel-init]
    Agent -->|Bootstrap Config| K8sConfig[/var/lib/keel/kubernetes]
    Init -->|Supervises| Agent
    Init -->|Supervises| Containerd[containerd]
    Init -->|Supervises| Kubelet[kubelet]
    Kubelet -->|Reads kubeconfig| K8sConfig
    Kubelet -->|CRI| Containerd
    Kubelet -->|Register + Heartbeat| K8s[K8s API Server]
    User -->|osctl bootstrap| Agent
```

**Bootstrap Flow:**
1.  **Administrator** runs `osctl bootstrap` with cluster credentials
2.  **keel-agent** generates and persists kubeconfig to `/var/lib/keel/kubernetes/`
3.  **keel-agent** signals kubelet restart
4.  **kubelet** reads kubeconfig and registers with the Kubernetes API server
5.  **Node** appears in the cluster and can accept workloads

**Ongoing Operation:**
1.  **Management**: The administrator talks to `keel-agent`.
2.  **Orchestration**: `kubelet` talks to the Kubernetes Control Plane.
3.  **Containers**: `kubelet` instructs `containerd` to run containers.
4.  **Lifecycle**: `keel-init` ensures everyone stays running.

## Security Architecture

*   **No Shell**: Shell access is physically impossible as no shell binary exists.
*   **Kernel Lockdown**: The kernel is configured to deny access to `/dev/mem` and `/dev/kmem`, preventing root from modifying kernel memory.
*   **mTLS**: The `keel-agent` only accepts connections from clients presenting a certificate signed by the cluster CA.
