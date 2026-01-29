# Networking

Networking in KeelOS is designed to be simple and predictable, removing the complexity of tools like NetworkManager, systemd-networkd, or netplan.

## Network Configuration

KeelOS provides API-driven network configuration that is applied at boot time, ensuring immutability and consistency.

### Configuration Methods

Network configuration can be managed through:
- **API**: Use the `ConfigureNetwork` RPC to set network configuration programmatically
- **CLI**: Use `osctl network` commands for interactive configuration
- **Config File**: Configuration is stored in `/var/lib/keel/network/config.json`

### Supported Interface Types

1. **DHCP**: Automatic IP configuration via DHCP
2. **Static IP**: Manual IP address, gateway, and MTU configuration
3. **VLAN**: 802.1Q VLAN tagging with parent interface
4. **Bonding**: Link aggregation (LACP, active-backup, balance-rr modes)

### Configuration Examples

#### Configure Static IP
```bash
osctl network config set \
  --interface eth0 \
  --ip 192.168.1.10/24 \
  --gateway 192.168.1.1 \
  --auto-reboot
```

#### Configure DHCP
```bash
osctl network config set \
  --interface eth0 \
  --dhcp \
  --auto-reboot
```

#### Configure DNS
```bash
osctl network dns set \
  --nameserver 8.8.8.8 \
  --nameserver 8.8.4.4 \
  --search example.com \
  --auto-reboot
```

#### View Configuration
```bash
osctl network config show
osctl network status
```

## Network Initialization

When `keel-init` starts, it performs network setup based on the configuration file:

1. **Loopback**: The `lo` interface is always brought up
2. **Configuration**: Reads `/var/lib/keel/network/config.json` and applies settings
3. **Fallback**: If no configuration exists, defaults to DHCP on `eth0`

Network changes require a reboot to take effect, maintaining KeelOS's immutable philosophy.

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

## API Reference

For detailed API documentation, see [Network Management API Reference](../reference/network-api.md).
