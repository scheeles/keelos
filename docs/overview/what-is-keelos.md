# What is KeelOS?

KeelOS is a next-generation, immutable, API-driven Linux distribution designed exclusively for hosting Kubernetes workloads. It represents a fundamental shift away from general-purpose operating systems, eliminating the traditional userspace to create a secure, minimal, and manageable foundation for modern infrastructure.

## The Problem with General-Purpose OSs

Traditional Linux distributions (Ubuntu, Debian, CentOS) are built for general-purpose computing. They come with a vast array of tools, services, and configurations—Shells, SSH, Systemd, package managers, interpreters (Python, Perl)—that are unnecessary and often detrimental for a dedicated Kubernetes node.

This complexity introduces:
*   **Security Risks**: Large attack surface due to unnecessary binaries and open ports (SSH).
*   **Drift**: Mutable filesystems allow configurations to diverge over time.
*   **Management Overload**: Patching, updating, and securing individual nodes is manual and error-prone.

## The KeelOS Solution

KeelOS is rebuilt from the ground up to be the perfect host for Kubernetes.

### 1. Immutable by Design
The entire operating system is delivered as a read-only SquashFS image. There is no package manager (`apt`, `yum`) and no writable system directories.
*   **Atomic Updates**: Updates are applied by downloading a new OS image and switching partitions. If the new image fails to boot or pass health checks, the system automatically rolls back to the previous version.
*   **No Drift**: Every node running version X.Y.Z is identical. Changes are lost on reboot unless explicitly persisted to designated data partitions.

### 2. API-Driven Management
KeelOS eliminates SSH and the console entirely. All management is performed via a highly secure, gRPC-based API.
*   **`osctl`**: A CLI tool that communicates with the API to manage the node from your laptop or CI/CD pipeline.
*   **Automation**: The API allows for programmatic control over updates, configuration, and health checks, enabling fully automated cluster management.

### 3. Minimalist & Lightweight
The OS image is under 100MB. It includes only what is strictly necessary to run Kubernetes:
*   **Kernel**: A minimal, hardened Linux kernel.
*   **`keel-init`**: A custom, single-binary PID 1 that replaces Systemd.
*   **`keel-agent`**: The gRPC management server.
*   **Container Runtime**: `containerd`.
*   **Kubernetes**: `kubelet`.

### 4. Secure by Default
By removing the shell and interpreters, KeelOS significantly raises the bar for attackers.
*   **No Shell**: There is no `/bin/bash` or `/bin/sh` to exploit.
*   **No Interpreters**: No Python, Perl, or Ruby means many attack tools cannot run.
*   **Kernel Lockdown**: The kernel enforces strict security policies.
*   **mTLS Everywhere**: All API communication is mutually authenticated and encrypted.

## Why KeelOS?

| Feature | Traditional Linux | KeelOS |
| :--- | :--- | :--- |
| **Management** | SSH, Ansible, Chef | gRPC API, `osctl` |
| **Updates** | `apt-get upgrade` (Package-based) | Atomic Image Swap (A/B Partition) |
| **Filesystem** | Writable (Mutable) | Read-Only (Immutable) |
| **Size** | ~500MB - 1GB+ | < 100MB |
| **PID 1** | Systemd | `keel-init` |
| **Shell Access** | Yes (SSH/Console) | No |

KeelOS provides a predictable, secure, and automated foundation for your Kubernetes clusters, allowing you to treat your nodes as cattle, not pets.
