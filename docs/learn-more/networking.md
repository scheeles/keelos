# Networking

Networking in KeelOS is designed to be simple and predictable, removing the complexity of tools like NetworkManager, systemd-networkd, or netplan.

## Network Initialization

When `keel-init` starts, it performs a minimal network setup:
1.  **Loopback**: The `lo` interface is brought up.
2.  **DHCP**: The system attempts to obtain an IP address via DHCP on the primary interface (`eth0`).

Currently, DHCP is the primary supported configuration method. Static IP configuration via the `keel-cmdline` or API is planned for future releases.

## Container Networking (CNI)

KeelOS is built to run Kubernetes, so it delegates most networking complexity to the **Container Network Interface (CNI)**.

*   **CNI Binaries**: KeelOS ships with standard CNI plugins (bridge, loopback, host-local, portmap) pre-installed in `/opt/cni/bin`.
*   **Configuration**: When you install a CNI provider (like Flannel, Calico, or Cilium) via Kubernetes manifests, it installs its configuration to `/etc/cni/net.d`.
*   **Persistence**: `/etc/cni` is effectively part of the writable overlay or persistent storage, allowing CNI configurations to survive reboots.

## Firewalling

KeelOS does not ship with `iptables` or `nftables` userspace tools directly manageable by the user.

*   **Kube-proxy**: The Kubernetes `kube-proxy` (running as a DaemonSet or binary) manages `iptables` rules for Service discovery.
*   **Host Security**: The kernel is configured to drop unsolicited incoming traffic on most ports, with exceptions for:
    *   **SSH**: Does not exist.
    *   **Port 50051**: The `keel-agent` management port.
    *   **Port 10250**: The Kubelet API.

## Proxy Support

If your cluster sits behind a corporate proxy, `keel-agent` and `containerd` can be configured to respect `HTTP_PROXY`, `HTTPS_PROXY`, and `NO_PROXY` environment variables. These are typically injected via kernel command-line arguments or cloud-init style userdata (planned).
