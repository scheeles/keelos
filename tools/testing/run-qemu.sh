#!/bin/bash
set -e

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BUILD_DIR="${PROJECT_ROOT}/build"
KERNEL="${BUILD_DIR}/kernel/bzImage"
INITRAMFS="${BUILD_DIR}/initramfs.cpio.gz"

# Host port for gRPC forwarding (default: 50052)
QEMU_HOST_PORT="${QEMU_HOST_PORT:-50052}"

# Disk image (default: sda.img in build dir)
QEMU_DISK="${QEMU_DISK:-${BUILD_DIR}/sda.img}"

# Check dependencies
if ! command -v qemu-system-x86_64 &> /dev/null; then
    echo "Error: qemu-system-x86_64 not found."
    exit 1
fi

if [ ! -f "${KERNEL}" ]; then
    echo "Warning: Kernel not found at ${KERNEL}. (Did you build it?)"
fi

echo ">>> Booting KeelOS in QEMU (port ${QEMU_HOST_PORT} -> 50051)..."
qemu-system-x86_64 \
    -kernel "${KERNEL}" \
    -initrd "${INITRAMFS}" \
    -m 4G \
    -smp 2 \
    -nographic \
    -append "console=ttyS0 quiet loglevel=3 init=/init panic=1 ${EXTRA_APPEND}" \
    -drive file="${QEMU_DISK}",format=raw \
    -net nic -net user,hostfwd=tcp::${QEMU_HOST_PORT}-:50051 \
    -no-reboot
