#!/bin/bash
set -e

KERNEL_VERSION="6.6.14"
KERNEL_URL="https://cdn.kernel.org/pub/linux/kernel/v6.x/linux-${KERNEL_VERSION}.tar.xz"
BUILD_DIR="/maticos/.cache/kernel"
SRC_DIR="${BUILD_DIR}/linux-${KERNEL_VERSION}"
OUTPUT_DIR="/maticos/build/kernel"

mkdir -p "${BUILD_DIR}"
mkdir -p "${OUTPUT_DIR}"

echo ">>> Checking for Kernel Source..."
if [ ! -d "${SRC_DIR}" ]; then
    echo "Downloading Kernel ${KERNEL_VERSION}..."
    wget -c "${KERNEL_URL}" -O "${BUILD_DIR}/linux.tar.xz"
    echo "Extracting..."
    tar -xf "${BUILD_DIR}/linux.tar.xz" -C "${BUILD_DIR}"
else
    echo "Source exists."
fi

cd "${SRC_DIR}"

echo ">>> Configuring Kernel..."
# Start with a minimal x86_64 configuration
make x86_64_defconfig

# Enforce our specific requirements (example: SquashFS, OverlayFS)
# In a real scenario, we would merge a fragment from /maticos/kernel/config/base.config
./scripts/config --enable CONFIG_SQUASHFS
./scripts/config --enable CONFIG_SQUASHFS_XZ
./scripts/config --enable CONFIG_OVERLAY_FS
./scripts/config --enable CONFIG_BLK_DEV_INITRD
./scripts/config --enable CONFIG_DEVTMPFS
./scripts/config --enable CONFIG_DEVTMPFS_MOUNT
./scripts/config --enable CONFIG_PVH # For QEMU/Firecracker

# Hardening (Breaks build if not careful, enabling basics)
./scripts/config --enable CONFIG_X86_64
./scripts/config --disable CONFIG_DEBUG_INFO

echo ">>> Building Kernel (bzImage)..."
make -j$(nproc) bzImage

echo ">>> Copying Artifacts..."
cp arch/x86/boot/bzImage "${OUTPUT_DIR}/bzImage"
echo "Kernel available at ${OUTPUT_DIR}/bzImage"
