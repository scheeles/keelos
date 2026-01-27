# Installing KeelOS

This guide covers installing KeelOS from pre-built release images.

> [!TIP]
> If you want to build KeelOS from source instead, see [Getting Started](./getting-started.md).

## Download Release Images

Download the latest release from [GitHub Releases](https://github.com/scheeles/keelos/releases).

| Format | Use Case | File |
|--------|----------|------|
| **ISO** | Bootable installer, live boot, bare metal | `keelos-<version>.iso` |
| **RAW** | Cloud VMs, generic hypervisors | `keelos-<version>.raw.gz` |
| **QCOW2** | KVM / libvirt | `keelos-<version>.qcow2` |
| **PXE** | Network boot | `keelos-<version>-pxe.tar.gz` |

### Verify Downloads

```bash
sha256sum -c SHA256SUMS
```

---

## Install from ISO

### Boot from ISO

1. Create a bootable USB or mount the ISO in your VM
2. Boot from the ISO
3. Select "KeelOS" from the GRUB menu

### Testing in QEMU

```bash
qemu-system-x86_64 \
  -cdrom keelos-0.1.0.iso \
  -m 2G \
  -smp 2 \
  -serial stdio
```

---

## Deploy to KVM/libvirt

### Using virt-install

```bash
# Download QCOW2 image
wget https://github.com/scheeles/keelos/releases/download/keelos-v0.1.0/keelos-0.1.0.qcow2

# Create VM
virt-install \
  --name keelos-node1 \
  --memory 2048 \
  --vcpus 2 \
  --disk path=keelos-0.1.0.qcow2,format=qcow2 \
  --import \
  --os-variant linux2024 \
  --network bridge=virbr0 \
  --graphics none \
  --console pty,target_type=serial
```

### Using virsh

```bash
# Create a VM definition
virsh define keelos-vm.xml
virsh start keelos-node1
```

---

## Deploy to Proxmox

1. Upload the RAW image to Proxmox storage:
   ```bash
   gunzip keelos-0.1.0.raw.gz
   qm importdisk <vmid> keelos-0.1.0.raw local-lvm
   ```

2. Attach the disk to your VM and set it as the boot device.

---

## PXE / Network Boot

Extract the PXE bundle:
```bash
tar -xzf keelos-0.1.0-pxe.tar.gz
```

Contents:
- `vmlinuz` - Linux kernel
- `initramfs.cpio.gz` - Initial ramdisk

### DHCP/TFTP Configuration

Configure your PXE server to serve these files. Example for dnsmasq:

```conf
dhcp-boot=pxelinux.0
enable-tftp
tftp-root=/srv/tftp
```

Create `/srv/tftp/pxelinux.cfg/default`:
```
DEFAULT keelos
LABEL keelos
  KERNEL vmlinuz
  APPEND initrd=initramfs.cpio.gz console=ttyS0,115200
```

---

## Hardware Requirements

| Resource | Minimum | Recommended |
|----------|---------|-------------|
| CPU | 1 core | 2+ cores |
| RAM | 1 GB | 2+ GB |
| Disk | 4 GB | 20+ GB |
| Network | 1 NIC | 1 NIC |

---

## First Boot

After booting KeelOS:

1. The system starts automatically (no login required)
2. `keel-agent` listens on port `50051`
3. Use `osctl` to manage the node remotely

```bash
osctl --addr <NODE_IP>:50051 status
```

See [Using osctl](./using-osctl.md) for complete CLI reference.

---

## Troubleshooting

### System doesn't boot

- Ensure UEFI/BIOS is configured for the correct boot mode
- Try the "KeelOS (Debug Mode)" option from the GRUB menu for verbose output

### Can't connect to keel-agent

- Verify the node's IP address
- Check that port 50051 is not blocked by a firewall
- Ensure the node has network connectivity
