#!/bin/bash
set -e

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TARGET_DIR="${PROJECT_ROOT}/target/x86_64-unknown-linux-musl/release"
OUTPUT_DIR="${PROJECT_ROOT}/build"
INITRAMFS_DIR="${OUTPUT_DIR}/initramfs"

mkdir -p "${INITRAMFS_DIR}/{bin,sbin,etc,proc,sys,usr/bin,usr/sbin}"
mkdir -p "${OUTPUT_DIR}"

echo ">>> Building matic-init..."
# In a real scenario, this runs inside the docker container
# cargo build --release --target x86_64-unknown-linux-musl --package matic-init

# Check if binary exists (assuming user ran build or we are mocking)
if [ ! -f "${TARGET_DIR}/matic-init" ]; then
    echo "WARNING: matic-init binary not found at ${TARGET_DIR}/matic-init"
    echo "Please run: cargo build --release --target x86_64-unknown-linux-musl"
    # Create dummy for testing if missing
    echo "Creating dummy init for structure..."
    touch "${INITRAMFS_DIR}/init"
    chmod +x "${INITRAMFS_DIR}/init"
else
    echo "Copying matic-init to /init..."
    cp "${TARGET_DIR}/matic-init" "${INITRAMFS_DIR}/init"
fi

# Create essential devices (if not using devtmpfs)
# sudo mknod -m 600 "${INITRAMFS_DIR}/dev/console" c 5 1
# sudo mknod -m 666 "${INITRAMFS_DIR}/dev/null" c 1 3

echo ">>> Packing initramfs.cpio.gz..."
cd "${INITRAMFS_DIR}"
find . -print0 | cpio --null -ov --format=newc | gzip > "${OUTPUT_DIR}/initramfs.cpio.gz"

echo "Initramfs created at ${OUTPUT_DIR}/initramfs.cpio.gz"
