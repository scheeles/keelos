#!/bin/bash
set -e

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BUILD_DIR="${PROJECT_ROOT}/build"

echo ">>> Verifying MaticOS Build Artifacts..."

# 1. Check if files exist
artifacts=(
    "${BUILD_DIR}/kernel/bzImage"
    "${BUILD_DIR}/initramfs.cpio.gz"
)

for art in "${artifacts[@]}"; do
    if [ ! -f "$art" ]; then
        echo "FAIL: Missing artifact $art"
        exit 1
    fi
    echo "OK: Found $(basename "$art")"
done

# 2. Verify Architecture of components (using file)
# Note: This checks the unpacked initramfs structure if available
if [ -d "${BUILD_DIR}/initramfs" ]; then
    echo ">>> Verifying binary architectures..."
    
    # Check for x86_64 ELF
    check_arch() {
        local file=$1
        if ! file "$file" | grep -q "ELF 64-bit LSB" || ! file "$file" | grep -q "x86-64"; then
            echo "FAIL: $file is not a 64-bit x86 binary"
            file "$file"
            exit 1
        fi
        echo "OK: $file architecture is correct"
    }

    check_arch "${BUILD_DIR}/initramfs/init"
    check_arch "${BUILD_DIR}/initramfs/usr/bin/matic-agent"
fi

echo ">>> ALL PHASE 1 & 2 ARTIFACTS VERIFIED <<<"
