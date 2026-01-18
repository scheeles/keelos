#!/bin/bash
set -e

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DISK_IMG="${PROJECT_ROOT}/build/sda.img"

if [ -f "$DISK_IMG" ]; then
    echo "Test disk already exists at $DISK_IMG"
    exit 0
fi

echo ">>> Creating test disk image (512MB)..."
mkdir -p "${PROJECT_ROOT}/build"
qemu-img create -f raw "$DISK_IMG" 512M

echo ">>> Partitioning disk with sfdisk..."
# Partition table (GPT)
# 1: 50M EFI
# 2: 200M ROOT_A
# 3: 200M ROOT_B
# 4: rest DATA
cat <<EOF | sfdisk "$DISK_IMG"
label: dos
size=50M, type=ef
size=200M, type=83
size=200M, type=83
type=83
EOF

echo ">>> Disk setup complete."
