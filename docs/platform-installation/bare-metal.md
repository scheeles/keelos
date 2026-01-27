# Bare Metal Installation

Installing KeelOS on physical hardware transforms a standard server into a dedicated Kubernetes appliance.

> [!WARNING]
> The Alpha release currently prioritizes QEMU support. Bare metal ISO generation is in active development. Specifically, the ISO generation tooling is planned for the Beta release.

## System Requirements

Ensure your hardware meets the [System Requirements](../getting-started/system-requirements.md).
*   **Architecture**: x86_64 (AMD64) only.
*   **Boot Mode**: UEFI is strongly recommended. Legacy BIOS support is limited.
*   **Secure Boot**: Must be **Disabled** for now (Kernel signing keys are not yet distributed to Microsoft).

## Installation Method (Planned)

The standard installation flow for bare metal will be booting from an ISO image.

### 1. Download ISO
Download the latest `keelos-amd64.iso` from the [GitHub Releases](https://github.com/scheeles/keelos/releases) page.

### 2. Create Boot Media
Write the ISO to a USB drive using a tool like `dd` or Etcher.

```bash
# Example (Linux/macOS)
sudo dd if=keelos-amd64.iso of=/dev/sdX bs=4M status=progress && sync
```

### 3. Boot & Install
1.  Insert the USB drive into the target machine.
2.  Boot the machine and select the USB drive from the UEFI boot menu.
3.   The system will boot into a "Live" mode running entirely in RAM.

### 4. Run Installer
Use `osctl` (bundled in the ISO) to install KeelOS to the local disk.

```bash
# Install to the first detected NVMe drive
osctl install-local --target /dev/nvme0n1
```

This process will:
1.  Partition the disk (EFI, Root A, Root B, Data).
2.  Install the GRUB/Limine bootloader.
3.  Copy the OS image to Root A.
4.  Initialize the Data partition.

### 5. Reboot
Remove the USB drive and reboot. The machine is now a KeelOS node.

## PXE Booting (Network Boot)

KeelOS is highly optimized for PXE booting, as the root filesystem is just a single SquashFS file.

**Requirements:**
*   DHCP Server (allowing custom boot options)
*   TFTP/HTTP Server (hosting kernel, initramfs, and squashfs)
*   iPXE (Recommended)

**iPXE Example Script:**
```text
#!ipxe
kernel http://boot.server/keelos/bzImage initrd=initramfs.cpio.gz
initrd http://boot.server/keelos/initramfs.cpio.gz
boot
```
