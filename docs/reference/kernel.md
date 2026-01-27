# Kernel Configuration

KeelOS uses a custom-compiled Linux kernel optimized for container workloads.

## Kernel Version

*   **Version**: 6.x (LTS)
*   **Patches**: Minimal patches applied for stability and build reproducibility.

## Build Configuration

The kernel is built with a minimalist configuration designed to reduce attack surface and boot time.

### Key Features
*   **Enabled**:
    *   `cgroups` (v2)
    *   `namespaces` (User, Net, PID, IPC, UTS, Mount)
    *   `overlayfs`
    *   `squashfs`
    *   `NET_SCHED` (Traffic control for CNI)
    *   `BPF` / `XDP` (For Cilium/eBPF support)

### Disabled Features
*   **Disabled**:
    *   Audio drivers / Sound support
    *   Legacy filesystem support (NTFS, FAT - except ESP)
    *   Wireless drivers (Wifi/Bluetooth) - *Server-class hardware focus*
    *   Graphics drivers (Direct rendering) - *Headless only*

## Command Line Arguments

The default kernel command line includes:

```text
console=ttyS0 quiet loglevel=3 init=/init panic=1
```

*   `console=ttyS0`: Serial console output (no VGA/HDMI output by default).
*   `init=/init`: Use `keel-init` as PID 1.
*   `panic=1`: Reboot immediately on kernel panic (HA behavior).
