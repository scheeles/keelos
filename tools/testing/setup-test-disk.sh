#!/bin/bash
set -e

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DISK_IMG="${PROJECT_ROOT}/build/sda.img"
FORCE="${1:-}"

if [ -f "$DISK_IMG" ] && [ "$FORCE" != "--force" ]; then
    echo "Test disk already exists at $DISK_IMG"
    echo "Use --force to recreate it."
    exit 0
fi

echo ">>> Creating test disk image (2GB)..."
mkdir -p "${PROJECT_ROOT}/build"
qemu-img create -f raw "$DISK_IMG" 2G

# Partition the disk image (only on Linux where sfdisk is available)
# Layout: 50M EFI, 200M ROOT_A, 200M ROOT_B, rest DATA
if command -v sfdisk &> /dev/null; then
    echo ">>> Partitioning disk with sfdisk..."
    cat <<EOF | sfdisk "$DISK_IMG"
label: dos
size=50M, type=ef
size=200M, type=83
size=200M, type=83
type=83
EOF
else
    echo ">>> Skipping partitioning (sfdisk not available on macOS)."
    echo "   The raw disk image will work for testing."
fi

echo ">>> Disk setup complete."
