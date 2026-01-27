# System Requirements

KeelOS is designed to be extremely lightweight, but it requires specific hardware features to provide its security and isolation guarantees.

## Supported Architectures

*   **AMD64 (x86_64)**: Primary supported architecture.
*   **ARM64 (aarch64)**: Planned / Experimental.

## Hardware Requirements

### Minimum
These are the absolute minimum resources required to boot KeelOS and run the `keel-agent` and `kubelet`.

*   **CPU**: 1 vCPU
*   **RAM**: 512 MB
*   **Storage**: 1 GB disk (for OS partitions + minor ephemeral storage)

### Recommended
For running actual Kubernetes workloads (Pods), you needed more resources.

*   **CPU**: 2+ vCPUs
*   **RAM**: 2 GB+ (depending on workload)
*   **Storage**: 10 GB+ (for container images and logs)

## Network Requirements

KeelOS nodes require network connectivity to function.

*   **DHCP**: By default, KeelOS attempts to obtain an IP address via DHCP on `eth0`.
*   **Outbound Access**:
    *   **Container Registry**: To pull container images (e.g., Docker Hub, Quay, GCR).
    *   **Kubernetes API**: Connectivity to the cluster control plane (port 6443).
*   **Inbound Access**:
    *   **Port 50051 (TCP)**: Exposed by `keel-agent` for gRPC management traffic. This should be firewalled to trusted management sources and `osctl` clients.
    *   **Port 10250 (TCP)**: Exposed by `kubelet` for Kubernetes API logs/exec.

## Boot Mode

*   **UEFI**: Recommended.
*   **BIOS (Legacy)**: Supported via GRUB/Syslinux (depending on build target).
