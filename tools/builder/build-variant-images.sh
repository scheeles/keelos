#!/bin/bash
set -e

# Build variant-specific image formats from existing build artifacts
#
# This script is called by build-variant.sh with the following env vars:
#   VARIANT_OUTPUT_FORMATS  - Space-separated list: iso raw qcow2 pxe vhd gcp
#   VARIANT_KERNEL_CMDLINE_EXTRA - Extra kernel command line parameters
#   VARIANT_GRUB_TIMEOUT    - GRUB menu timeout in seconds
#   VARIANT_OUTPUT_DIR      - Where to write output images
#   VARIANT_NAME            - Variant name (used in filenames)
#
# It can also be run standalone with the same env vars set.

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BUILD_DIR="${PROJECT_ROOT}/build"
VERSION="${1:-dev}"

# Defaults if not called from build-variant.sh
OUTPUT_FORMATS="${VARIANT_OUTPUT_FORMATS:-iso raw qcow2 pxe}"
CMDLINE_EXTRA="${VARIANT_KERNEL_CMDLINE_EXTRA:-}"
GRUB_TIMEOUT="${VARIANT_GRUB_TIMEOUT:-5}"
OUTPUT_DIR="${VARIANT_OUTPUT_DIR:-${BUILD_DIR}}"
VARIANT="${VARIANT_NAME:-base}"

# Required source files
KERNEL="${BUILD_DIR}/kernel/bzImage"
INITRAMFS="${BUILD_DIR}/initramfs.cpio.gz"
DISK_IMG="${BUILD_DIR}/sda.img"

# Use variant-specific kernel if available
if [ -f "${OUTPUT_DIR}/kernel/bzImage" ]; then
    KERNEL="${OUTPUT_DIR}/kernel/bzImage"
fi

echo ">>> Building KeelOS ${VARIANT} images (version: ${VERSION})..."
echo "    Formats: ${OUTPUT_FORMATS}"

# Verify essential source files exist
for f in "$KERNEL" "$INITRAMFS"; do
    if [[ ! -f "$f" ]]; then
        echo "ERROR: Required file not found: $f"
        echo "Please run the full build first."
        exit 1
    fi
done

mkdir -p "${OUTPUT_DIR}"

# =============================================================================
# Helper: filename prefix
# =============================================================================
PREFIX="keelos-${VERSION}-${VARIANT}"

# =============================================================================
# ISO Image (GRUB bootable)
# =============================================================================
if echo "${OUTPUT_FORMATS}" | grep -qw "iso"; then
    echo ">>> Building ISO image..."
    ISO_DIR="${OUTPUT_DIR}/iso-staging"
    rm -rf "${ISO_DIR}"
    mkdir -p "${ISO_DIR}/boot/grub"

    cp "${KERNEL}" "${ISO_DIR}/boot/vmlinuz"
    cp "${INITRAMFS}" "${ISO_DIR}/boot/initramfs.cpio.gz"

    cat > "${ISO_DIR}/boot/grub/grub.cfg" << GRUB_EOF
set default=0
set timeout=${GRUB_TIMEOUT}

menuentry "KeelOS (${VARIANT})" {
    linux /boot/vmlinuz console=ttyS0,115200 console=tty0 ${CMDLINE_EXTRA}
    initrd /boot/initramfs.cpio.gz
}

menuentry "KeelOS (${VARIANT}, Debug Mode)" {
    linux /boot/vmlinuz console=ttyS0,115200 console=tty0 debug loglevel=7 ${CMDLINE_EXTRA}
    initrd /boot/initramfs.cpio.gz
}
GRUB_EOF

    grub-mkrescue -o "${OUTPUT_DIR}/${PREFIX}.iso" "${ISO_DIR}"
    rm -rf "${ISO_DIR}"
    echo "    Created: ${PREFIX}.iso"
fi

# =============================================================================
# RAW Disk Image (compressed)
# =============================================================================
if echo "${OUTPUT_FORMATS}" | grep -qw "raw"; then
    echo ">>> Creating compressed RAW image..."
    if [[ -f "${DISK_IMG}" ]]; then
        gzip -c "${DISK_IMG}" > "${OUTPUT_DIR}/${PREFIX}.raw.gz"
        echo "    Created: ${PREFIX}.raw.gz"
    else
        echo "    WARNING: Disk image not found at ${DISK_IMG}, skipping RAW format"
    fi
fi

# =============================================================================
# QCOW2 Image (KVM/libvirt)
# =============================================================================
if echo "${OUTPUT_FORMATS}" | grep -qw "qcow2"; then
    echo ">>> Converting to QCOW2..."
    if [[ -f "${DISK_IMG}" ]]; then
        qemu-img convert -f raw -O qcow2 -c "${DISK_IMG}" "${OUTPUT_DIR}/${PREFIX}.qcow2"
        echo "    Created: ${PREFIX}.qcow2"
    else
        echo "    WARNING: Disk image not found at ${DISK_IMG}, skipping QCOW2 format"
    fi
fi

# =============================================================================
# PXE Bundle (kernel + initramfs for network boot)
# =============================================================================
if echo "${OUTPUT_FORMATS}" | grep -qw "pxe"; then
    echo ">>> Creating PXE bundle..."
    PXE_DIR="${OUTPUT_DIR}/pxe-staging"
    mkdir -p "${PXE_DIR}"
    cp "${KERNEL}" "${PXE_DIR}/vmlinuz"
    cp "${INITRAMFS}" "${PXE_DIR}/initramfs.cpio.gz"

    # Include a default pxelinux config for convenience
    cat > "${PXE_DIR}/pxelinux.cfg" << PXE_EOF
# KeelOS PXE Boot Configuration (${VARIANT} variant)
# Copy vmlinuz and initramfs.cpio.gz to your TFTP server root.
# Point your DHCP server's next-server to the TFTP server.
#
# Example pxelinux.cfg/default:
DEFAULT keelos
LABEL keelos
    KERNEL vmlinuz
    INITRD initramfs.cpio.gz
    APPEND console=ttyS0,115200 console=tty0 ${CMDLINE_EXTRA}
PXE_EOF

    tar -czvf "${OUTPUT_DIR}/${PREFIX}-pxe.tar.gz" -C "${PXE_DIR}" .
    rm -rf "${PXE_DIR}"
    echo "    Created: ${PREFIX}-pxe.tar.gz"
fi

# =============================================================================
# VHD Image (Azure)
# =============================================================================
if echo "${OUTPUT_FORMATS}" | grep -qw "vhd"; then
    echo ">>> Creating VHD image (Azure)..."
    if [[ -f "${DISK_IMG}" ]]; then
        # Azure requires fixed-size VHD with size aligned to 1MB
        RAW_SIZE=$(stat -c%s "${DISK_IMG}")
        # Round up to nearest MB
        MB=$((1024 * 1024))
        ALIGNED_SIZE=$(( (RAW_SIZE + MB - 1) / MB * MB ))

        # Create aligned raw copy
        VHD_RAW="${OUTPUT_DIR}/${PREFIX}-azure.raw"
        cp "${DISK_IMG}" "${VHD_RAW}"
        truncate -s "${ALIGNED_SIZE}" "${VHD_RAW}"

        # Convert to VHD (fixed format required by Azure)
        qemu-img convert -f raw -O vpc -o subformat=fixed,force_size \
            "${VHD_RAW}" "${OUTPUT_DIR}/${PREFIX}.vhd"
        rm -f "${VHD_RAW}"
        echo "    Created: ${PREFIX}.vhd"
    else
        echo "    WARNING: Disk image not found at ${DISK_IMG}, skipping VHD format"
    fi
fi

# =============================================================================
# GCP Image (tar.gz of raw disk named disk.raw)
# =============================================================================
if echo "${OUTPUT_FORMATS}" | grep -qw "gcp"; then
    echo ">>> Creating GCP image..."
    if [[ -f "${DISK_IMG}" ]]; then
        # GCP requires a tar.gz containing a single file named "disk.raw"
        GCP_DIR="${OUTPUT_DIR}/gcp-staging"
        mkdir -p "${GCP_DIR}"
        cp "${DISK_IMG}" "${GCP_DIR}/disk.raw"
        tar -czvf "${OUTPUT_DIR}/${PREFIX}-gcp.tar.gz" -C "${GCP_DIR}" disk.raw
        rm -rf "${GCP_DIR}"
        echo "    Created: ${PREFIX}-gcp.tar.gz"
    else
        echo "    WARNING: Disk image not found at ${DISK_IMG}, skipping GCP format"
    fi
fi

# =============================================================================
# Summary
# =============================================================================
echo ""
echo ">>> Build complete! ${VARIANT} variant artifacts:"
echo ""
ls -lh "${OUTPUT_DIR}"/${PREFIX}* 2>/dev/null || echo "  (no artifacts found)"
