#!/bin/bash
set -e

# Build all OS image formats from existing build artifacts
# Must be run inside the builder container after the main build

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BUILD_DIR="${PROJECT_ROOT}/build"
VERSION="${1:-dev}"

# Required source files
KERNEL="${BUILD_DIR}/kernel/bzImage"
INITRAMFS="${BUILD_DIR}/initramfs.cpio.gz"
DISK_IMG="${BUILD_DIR}/sda.img"

echo ">>> Building MaticOS images (version: ${VERSION})..."

# Verify source files exist
for f in "$KERNEL" "$INITRAMFS" "$DISK_IMG"; do
    if [[ ! -f "$f" ]]; then
        echo "ERROR: Required file not found: $f"
        echo "Please run the full build first."
        exit 1
    fi
done

# =============================================================================
# ISO Image (using existing iso-build.sh)
# =============================================================================
echo ">>> Building ISO..."
"${PROJECT_ROOT}/tools/builder/iso-build.sh"
if [[ -f "${BUILD_DIR}/maticos.iso" ]]; then
    mv "${BUILD_DIR}/maticos.iso" "${BUILD_DIR}/maticos-${VERSION}.iso"
    echo "    Created: maticos-${VERSION}.iso"
fi

# =============================================================================
# RAW Disk Image (compressed)
# =============================================================================
echo ">>> Creating compressed RAW image..."
# The sda.img is already a raw disk image, just compress it
gzip -c "${DISK_IMG}" > "${BUILD_DIR}/maticos-${VERSION}.raw.gz"
echo "    Created: maticos-${VERSION}.raw.gz"

# =============================================================================
# QCOW2 Image (KVM/libvirt)
# =============================================================================
echo ">>> Converting to QCOW2..."
qemu-img convert -f raw -O qcow2 -c "${DISK_IMG}" "${BUILD_DIR}/maticos-${VERSION}.qcow2"
echo "    Created: maticos-${VERSION}.qcow2"

# =============================================================================
# Kernel Bundle (for PXE/netboot)
# =============================================================================
echo ">>> Creating kernel bundle for PXE..."
mkdir -p "${BUILD_DIR}/pxe"
cp "${KERNEL}" "${BUILD_DIR}/pxe/vmlinuz"
cp "${INITRAMFS}" "${BUILD_DIR}/pxe/initramfs.cpio.gz"
tar -czvf "${BUILD_DIR}/maticos-${VERSION}-pxe.tar.gz" -C "${BUILD_DIR}/pxe" .
rm -rf "${BUILD_DIR}/pxe"
echo "    Created: maticos-${VERSION}-pxe.tar.gz"

# =============================================================================
# Summary
# =============================================================================
echo ""
echo ">>> Build complete! Release artifacts:"
echo ""
ls -lh "${BUILD_DIR}"/maticos-${VERSION}*
