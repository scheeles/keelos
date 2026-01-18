#!/bin/bash
set -e

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TARGET_DIR="${PROJECT_ROOT}/target/x86_64-unknown-linux-musl/release"
OUTPUT_DIR="${PROJECT_ROOT}/build"
INITRAMFS_DIR="${OUTPUT_DIR}/initramfs"

rm -rf "${INITRAMFS_DIR}"
mkdir -p "${INITRAMFS_DIR}"

mkdir -p "${INITRAMFS_DIR}/bin"
mkdir -p "${INITRAMFS_DIR}/sbin"
mkdir -p "${INITRAMFS_DIR}/etc"
mkdir -p "${INITRAMFS_DIR}/proc"
mkdir -p "${INITRAMFS_DIR}/sys"
mkdir -p "${INITRAMFS_DIR}/usr/bin"
mkdir -p "${INITRAMFS_DIR}/usr/sbin"
mkdir -p "${INITRAMFS_DIR}/opt/cni/bin"
mkdir -p "${INITRAMFS_DIR}/var/lib/containerd"
mkdir -p "${INITRAMFS_DIR}/run/containerd"
mkdir -p "${INITRAMFS_DIR}/etc/containerd"
mkdir -p "${OUTPUT_DIR}"

echo ">>> Copying external binaries..."
# These come from the build container's /usr/local/bin or /usr/local/sbin
cp -L /usr/local/bin/containerd* "${INITRAMFS_DIR}/usr/bin/"
cp -L /usr/local/bin/ctr "${INITRAMFS_DIR}/usr/bin/"
cp -L /usr/local/sbin/runc "${INITRAMFS_DIR}/usr/sbin/"
cp -rL /opt/cni/bin/* "${INITRAMFS_DIR}/opt/cni/bin/"
cp -L "${PROJECT_ROOT}/tools/builder/containerd-config.toml" "${INITRAMFS_DIR}/etc/containerd/config.toml"

echo ">>> Copying GLIBC libraries (for dynamic Kubelet)..."
mkdir -p "${INITRAMFS_DIR}/lib64"
mkdir -p "${INITRAMFS_DIR}/lib"
# Interpreter (Must match Requesting program interpreter path)
cp -L /usr/x86_64-linux-gnu/lib/ld-linux-x86-64.so.2 "${INITRAMFS_DIR}/lib64/"
# Dependencies (Cross-compiler paths)
cp -L /usr/x86_64-linux-gnu/lib/libc.so.6 "${INITRAMFS_DIR}/lib/"
cp -L /usr/x86_64-linux-gnu/lib/libpthread.so.0 "${INITRAMFS_DIR}/lib/"
cp -L /usr/x86_64-linux-gnu/lib/libresolv.so.2 "${INITRAMFS_DIR}/lib/"
# Optional but recommended
if [ -f /usr/x86_64-linux-gnu/lib/libdl.so.2 ]; then cp -L /usr/x86_64-linux-gnu/lib/libdl.so.2 "${INITRAMFS_DIR}/lib/"; fi
if [ -f /usr/x86_64-linux-gnu/lib/libm.so.6 ]; then cp -L /usr/x86_64-linux-gnu/lib/libm.so.6 "${INITRAMFS_DIR}/lib/"; fi

cp -L "${PROJECT_ROOT}/tools/builder/containerd-config.toml" "${INITRAMFS_DIR}/etc/containerd/config.toml"

echo ">>> Copying Kubernetes binaries..."
mkdir -p "${INITRAMFS_DIR}/var/lib/kubelet"
mkdir -p "${INITRAMFS_DIR}/etc/kubernetes/manifests"
cp -L /usr/local/bin/kubelet "${INITRAMFS_DIR}/usr/bin/"
cp -L "${PROJECT_ROOT}/tools/builder/kubelet-config.yaml" "${INITRAMFS_DIR}/etc/kubernetes/kubelet-config.yaml"

echo ">>> Building matic-init..."
# In a real scenario, this runs inside the docker container
# cargo build --release --target x86_64-unknown-linux-musl --package matic-init

# Check if binary exists (assuming user ran build or we are mocking)
if [ ! -f "${TARGET_DIR}/matic-init" ]; then
    echo "ERROR: matic-init binary not found at ${TARGET_DIR}/matic-init"
    echo "Please run: cargo build --release --target x86_64-unknown-linux-musl"
    exit 1
else
    echo "Copying matic-init to /init..."
    cp "${TARGET_DIR}/matic-init" "${INITRAMFS_DIR}/init"

    if [ -f "${TARGET_DIR}/matic-agent" ]; then
        echo "Copying matic-agent to /usr/bin/matic-agent..."
        cp "${TARGET_DIR}/matic-agent" "${INITRAMFS_DIR}/usr/bin/matic-agent"
    fi
fi

# Create essential devices (if not using devtmpfs)
# sudo mknod -m 600 "${INITRAMFS_DIR}/dev/console" c 5 1
# sudo mknod -m 666 "${INITRAMFS_DIR}/dev/null" c 1 3

echo ">>> Packing initramfs.cpio.gz..."
cd "${INITRAMFS_DIR}"
find . -print0 | cpio --null -ov --format=newc | gzip > "${OUTPUT_DIR}/initramfs.cpio.gz"

echo "Initramfs created at ${OUTPUT_DIR}/initramfs.cpio.gz"
