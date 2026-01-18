#!/bin/bash
set -e

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BUILD_DIR="${PROJECT_ROOT}/build"
KERNEL="${BUILD_DIR}/kernel/bzImage"
INITRAMFS="${BUILD_DIR}/initramfs.cpio.gz"

# Check dependencies
if ! command -v qemu-system-x86_64 &> /dev/null; then
    echo "Error: qemu-system-x86_64 not found."
    exit 1
fi

if [ ! -f "${KERNEL}" ]; then
    echo "Warning: Kernel not found at ${KERNEL}. (Did you build it?)"
fi

echo ">>> Booting MaticOS in QEMU..."
qemu-system-x86_64 \
    -kernel "${KERNEL}" \
    -initrd "${INITRAMFS}" \
    -m 1G \
    -nographic \
    -append "console=ttyS0 quiet loglevel=3 init=/init panic=1 ${EXTRA_APPEND}" \
    -drive file="${BUILD_DIR}/sda.img",format=raw \
    -net nic -net user,hostfwd=tcp::50052-:50051 \
    -no-reboot
