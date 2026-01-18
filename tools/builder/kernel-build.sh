#!/bin/bash
set -e

KERNEL_VERSION="6.6.14"
KERNEL_URL="https://cdn.kernel.org/pub/linux/kernel/v6.x/linux-${KERNEL_VERSION}.tar.xz"
CACHE_DIR="/maticos/.cache/kernel"
# Build in ephemeral container FS which is case-sensitive (fixes Mac host mount issues)
BUILD_DIR="/tmp/kernel-build"
SRC_DIR="${BUILD_DIR}/linux-${KERNEL_VERSION}"
OUTPUT_DIR="/maticos/build/kernel"

mkdir -p "${CACHE_DIR}"
mkdir -p "${BUILD_DIR}"
mkdir -p "${OUTPUT_DIR}"

echo ">>> Checking for Kernel Source Tarball..."
if [ ! -f "${CACHE_DIR}/linux.tar.xz" ]; then
    echo "Downloading Kernel ${KERNEL_VERSION}..."
    wget -c "${KERNEL_URL}" -O "${CACHE_DIR}/linux.tar.xz"
fi

echo ">>> Extracting Source to Ephemeral Build Dir..."
# We extract every time to ensure a clean source tree on a proper filesystem
if [ -d "${SRC_DIR}" ]; then
    rm -rf "${SRC_DIR}"
fi
tar -xf "${CACHE_DIR}/linux.tar.xz" -C "${BUILD_DIR}"

cd "${SRC_DIR}"

echo ">>> Configuring Kernel..."
# Clean up any potential stale objects from wrong architecture
make mrproper

# Start with a minimal x86_64 configuration
make ARCH=x86_64 CROSS_COMPILE=x86_64-linux-gnu- x86_64_defconfig

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

./scripts/config --disable CONFIG_DEBUG_INFO

# Update config to resolve any new dependencies non-interactively
make ARCH=x86_64 CROSS_COMPILE=x86_64-linux-gnu- olddefconfig

echo ">>> Building Kernel (bzImage)..."
make -j$(nproc) ARCH=x86_64 CROSS_COMPILE=x86_64-linux-gnu- bzImage

echo ">>> Copying Artifacts..."
cp arch/x86/boot/bzImage "${OUTPUT_DIR}/bzImage"
echo "Kernel available at ${OUTPUT_DIR}/bzImage"
