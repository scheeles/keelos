# Network Management API Reference

This document describes the Network Management API for KeelOS nodes.

## Overview

The Network Management API provides three RPC methods for configuring and querying network settings:

- **ConfigureNetwork**: Save network configuration to disk
- **GetNetworkConfig**: Retrieve current network configuration
- **GetNetworkStatus**: Query runtime network interface status

All network configuration changes require a reboot to take effect, maintaining KeelOS's immutable philosophy.

## RPC Methods

### ConfigureNetwork

Saves network configuration to `/var/lib/keel/network/config.json`. Changes take effect on next boot.

**Request**: `ConfigureNetworkRequest`
```protobuf
message ConfigureNetworkRequest {
  repeated NetworkInterface interfaces = 1;
  DnsConfig dns = 2;
  repeated NetworkRoute routes = 3;
  bool auto_reboot = 4;
}
```

**Response**: `ConfigureNetworkResponse`
```protobuf
message ConfigureNetworkResponse {
  bool success = 1;
  string message = 2;
  bool reboot_required = 3;
}
```

**Example (gRPC)**:
```go
resp, err := client.ConfigureNetwork(ctx, &node.ConfigureNetworkRequest{
    Interfaces: []*node.NetworkInterface{
        {
            Name: "eth0",
            Config: &node.NetworkInterface_Static{
                Static: &node.StaticConfig{
                    Ipv4Address: "192.168.1.10/24",
                    Gateway: "192.168.1.1",
                    Mtu: 1500,
                },
            },
        },
    },
    Dns: &node.DnsConfig{
        Nameservers: []string{"8.8.8.8", "8.8.4.4"},
        SearchDomains: []string{"example.com"},
    },
    AutoReboot: false,
})
```

**Example (osctl)**:
```bash
# Static IP
osctl network config set \
  --interface eth0 \
  --ip 192.168.1.10/24 \
  --gateway 192.168.1.1 \
  --auto-reboot

# DHCP
osctl network config set \
  --interface eth0 \
  --dhcp \
  --auto-reboot

# DNS
osctl network dns set \
  --nameserver 8.8.8.8 \
  --nameserver 8.8.4.4 \
  --auto-reboot
```

### GetNetworkConfig

Retrieves the current network configuration from `/var/lib/keel/network/config.json`.

**Request**: `GetNetworkConfigRequest`
```protobuf
message GetNetworkConfigRequest {}
```

**Response**: `GetNetworkConfigResponse`
```protobuf
message GetNetworkConfigResponse {
  repeated NetworkInterface interfaces = 1;
  DnsConfig dns = 2;
  repeated NetworkRoute routes = 3;
}
```

**Example (osctl)**:
```bash
osctl network config show
```

**Example Output**:
```
📡 Network Configuration:

Interface: eth0
  Type: Static
  IP: 192.168.1.10/24
  Gateway: 192.168.1.1
  MTU: 1500

DNS Configuration:
  Nameservers: 8.8.8.8, 8.8.4.4
  Search domains: example.com
```

### GetNetworkStatus

Queries runtime network interface status from the system.

**Request**: `GetNetworkStatusRequest`
```protobuf
message GetNetworkStatusRequest {}
```

**Response**: `GetNetworkStatusResponse`
```protobuf
message GetNetworkStatusResponse {
  repeated InterfaceStatus interfaces = 1;
}
```

**Example (osctl)**:
```bash
osctl network status
```

**Example Output**:
```
🌐 Network Status:

🟢 eth0 (up)
  MAC: 52:54:00:12:34:56
  MTU: 1500
  IPv4: 192.168.1.10/24
  RX: 125.45 MB (98234 packets, 0 errors)
  TX: 67.89 MB (54321 packets, 0 errors)

🟢 lo (up)
  MAC: 00:00:00:00:00:00
  MTU: 65536
  IPv4: 127.0.0.1/8
```

## Message Types

### NetworkInterface

Represents a network interface configuration.

```protobuf
message NetworkInterface {
  string name = 1;
  oneof config {
    DhcpConfig dhcp = 2;
    StaticConfig static = 3;
    VlanConfig vlan = 4;
    BondConfig bond = 5;
  }
}
```

**Fields**:
- `name`: Interface name (e.g., "eth0", "ens3")
- `config`: One of DHCP, Static, VLAN, or Bond configuration

### DhcpConfig

DHCP configuration for automatic IP assignment.

```protobuf
message DhcpConfig {
  bool enabled = 1;
}
```

### StaticConfig

Static IP configuration.

```protobuf
message StaticConfig {
  string ipv4_address = 1;  // CIDR notation (e.g., "192.168.1.10/24")
  string gateway = 2;        // Gateway IP address
  uint32 mtu = 3;            // MTU (default: 1500)
}
```

**Validation**:
- `ipv4_address` must be valid CIDR notation
- `gateway` must be valid IPv4 address
- `mtu` typically 1280-9000

### VlanConfig

VLAN (802.1Q) configuration.

```protobuf
message VlanConfig {
  string parent = 1;         // Parent interface name
  uint32 vlan_id = 2;        // VLAN ID (1-4094)
  oneof ip_config {
    DhcpConfig dhcp = 3;
    StaticConfig static = 4;
  }
}
```

**Validation**:
- `vlan_id` must be 1-4094
- `parent` must be a valid interface name

### BondConfig

Link aggregation (bonding) configuration.

```protobuf
message BondConfig {
  string mode = 1;           // Bonding mode
  repeated string slaves = 2; // Slave interface names
  oneof ip_config {
    DhcpConfig dhcp = 3;
    StaticConfig static = 4;
  }
}
```

**Supported Modes**:
- `802.3ad`: LACP (IEEE 802.3ad)
- `active-backup`: Active-backup policy
- `balance-rr`: Round-robin policy

### DnsConfig

DNS resolver configuration.

```protobuf
message DnsConfig {
  repeated string nameservers = 1;    // DNS server IPs
  repeated string search_domains = 2; // Search domains
}
```

**Validation**:
- `nameservers` must be valid IPv4/IPv6 addresses
- At least one nameserver recommended

### NetworkRoute

Custom routing table entry.

```protobuf
message NetworkRoute {
  string destination = 1;  // Destination network (CIDR)
  string gateway = 2;      // Gateway IP
  uint32 metric = 3;       // Route metric (optional)
}
```

### InterfaceStatus

Runtime status of a network interface.

```protobuf
message InterfaceStatus {
  string name = 1;
  string state = 2;                    // "up" or "down"
  string mac_address = 3;
  uint32 mtu = 4;
  repeated string ipv4_addresses = 5;
  InterfaceStatistics statistics = 6;
}
```

### InterfaceStatistics

Network interface statistics.

```protobuf
message InterfaceStatistics {
  uint64 rx_bytes = 1;
  uint64 tx_bytes = 2;
  uint64 rx_packets = 3;
  uint64 tx_packets = 4;
  uint64 rx_errors = 5;
  uint64 tx_errors = 6;
}
```

## Configuration File Format

Network configuration is stored in `/var/lib/keel/network/config.json`:

```json
{
  "interfaces": [
    {
      "name": "eth0",
      "interface_type": {
        "Static": {
          "ipv4_address": "192.168.1.10/24",
          "gateway": "192.168.1.1",
          "mtu": 1500
        }
      }
    }
  ],
  "dns": {
    "nameservers": ["8.8.8.8", "8.8.4.4"],
    "search_domains": ["example.com"]
  },
  "routes": []
}
```

## Boot-time Application

Network configuration is applied by `keel-init` during system boot:

1. `keel-init` reads `/var/lib/keel/network/config.json`
2. Applies configuration using the `ip` command
3. Writes DNS configuration to `/etc/resolv.conf`
4. Falls back to DHCP on `eth0` if no configuration exists

## Design Decisions

### Reboot Required

Network configuration changes require a reboot because:
- Maintains immutability principle
- Ensures clean state on every boot
- Prevents runtime network disruptions
- Simplifies error handling and rollback

### Configuration Validation

All configurations are validated before saving:
- IP addresses must be valid CIDR notation
- Interface names must be 1-15 characters
- VLAN IDs must be 1-4094
- Bonding modes must be supported

Invalid configurations are rejected with descriptive error messages.

### Fallback Behavior

If `/var/lib/keel/network/config.json` doesn't exist or is invalid:
- System falls back to DHCP on `eth0`
- Loopback interface is always configured
- System remains accessible for troubleshooting

## Error Handling

### Common Errors

**Invalid IP Address**:
```
❌ Configuration failed: Invalid IP address: not-an-ip
```

**Invalid Interface Name**:
```
❌ Configuration failed: Invalid interface name: this-name-is-way-too-long
```

**Missing Required Fields**:
```
Error: Either --dhcp or --ip must be specified
```

### Validation

The API performs comprehensive validation:
- IP address format (CIDR notation)
- Gateway reachability (same subnet as IP)
- Interface name length and characters
- VLAN ID range (1-4094)
- Bonding mode support

## Security Considerations

- Network configuration requires mTLS authentication
- Configuration file is world-readable but only writable by root
- DNS configuration is applied to `/etc/resolv.conf` (read-only filesystem)
- No runtime network changes prevent unauthorized modifications

## Future Enhancements

Planned features (not yet implemented):
- IPv6 support
- Network plugin framework
- Immediate application of network changes (without reboot)
- Automatic rollback on network failures
- Network configuration templates
- DHCP options customization

## See Also

- [Networking Overview](../learn-more/networking.md)
- [KeelOS Architecture](../learn-more/architecture.md)
- [osctl CLI Reference](./osctl.md)
