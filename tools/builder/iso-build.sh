#!/bin/bash
set -e

# This script builds a bootable ISO image for KeelOS
# Must be run inside the builder container after kernel and initramfs are built

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BUILD_DIR="${PROJECT_ROOT}/build"
ISO_DIR="${BUILD_DIR}/iso"
ISO_OUTPUT="${BUILD_DIR}/keelos.iso"

# Check required files exist
if [ ! -f "${BUILD_DIR}/kernel/bzImage" ]; then
    echo "ERROR: Kernel not found at ${BUILD_DIR}/kernel/bzImage"
    echo "Please run kernel-build.sh first"
    exit 1
fi

if [ ! -f "${BUILD_DIR}/initramfs.cpio.gz" ]; then
    echo "ERROR: Initramfs not found at ${BUILD_DIR}/initramfs.cpio.gz"
    echo "Please run initramfs-build.sh first"
    exit 1
fi

echo ">>> Creating ISO directory structure..."
rm -rf "${ISO_DIR}"
mkdir -p "${ISO_DIR}/boot/grub"

echo ">>> Copying kernel and initramfs..."
cp "${BUILD_DIR}/kernel/bzImage" "${ISO_DIR}/boot/vmlinuz"
cp "${BUILD_DIR}/initramfs.cpio.gz" "${ISO_DIR}/boot/initramfs.cpio.gz"

echo ">>> Creating GRUB configuration..."
cat > "${ISO_DIR}/boot/grub/grub.cfg" << 'EOF'
set default=0
set timeout=5

menuentry "KeelOS" {
    linux /boot/vmlinuz console=ttyS0,115200 console=tty0
    initrd /boot/initramfs.cpio.gz
}

menuentry "KeelOS (Debug Mode)" {
    linux /boot/vmlinuz console=ttyS0,115200 console=tty0 debug loglevel=7
    initrd /boot/initramfs.cpio.gz
}
EOF

echo ">>> Building ISO with grub-mkrescue..."
grub-mkrescue -o "${ISO_OUTPUT}" "${ISO_DIR}"

# Verify the ISO was created
if [ -f "${ISO_OUTPUT}" ]; then
    ISO_SIZE=$(du -h "${ISO_OUTPUT}" | cut -f1)
    echo ">>> ISO created successfully: ${ISO_OUTPUT} (${ISO_SIZE})"
else
    echo "ERROR: Failed to create ISO"
    exit 1
fi
