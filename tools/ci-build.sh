#!/bin/bash
set -e

# This script is intended to be run INSIDE the builder container.
# It builds the OS components and runs tests.

echo ">>> Starting CI Build Process..."

# 1. Build Rust Binaries (Init, Agent, API)
echo ">>> Building Rust components (musl)..."
cargo build --release --target x86_64-unknown-linux-musl --workspace

# 2. Run Unit Tests
echo ">>> Running Rust tests..."
cargo test --workspace

# 3. Build Kernel
# This script downloads/builds the kernel if not present
echo ">>> Building Kernel..."
./tools/builder/kernel-build.sh

# 4. Build Initramfs
# Packs the Rust binaries and other system files
echo ">>> Building Initramfs..."
./tools/builder/initramfs-build.sh

# 5. Run Integration/Boot Test
echo ">>> Running QEMU Boot Test..."
./tools/testing/test-boot.sh

echo ">>> CI Build Complete!"
