#!/bin/bash
set -e

# This script is intended to be run INSIDE the builder container.
# It only builds components - tests run separately in parallel CI jobs.

echo ">>> Starting CI Build Process..."

# =============================================================================
# BUILD PHASE ONLY
# =============================================================================

# 1. Build Rust Binaries (Init, Agent, API, osctl)
echo ">>> Building Rust components (musl)..."
cargo build --release --target x86_64-unknown-linux-musl --workspace

# Copy osctl binary to build directory for test jobs
echo ">>> Copying osctl to build directory..."
mkdir -p /keelos/build
cp /keelos/target/x86_64-unknown-linux-musl/release/osctl /keelos/build/osctl
chmod +x /keelos/build/osctl

# 2. Build Kernel (skipped if cached)
echo ">>> Building Kernel..."
./tools/builder/kernel-build.sh

# 3. Build Initramfs
echo ">>> Building Initramfs..."
./tools/builder/initramfs-build.sh

# 4. Build ISO
echo ">>> Building ISO..."
./tools/builder/iso-build.sh

# 5. Setup Test Disk (needed for QEMU tests)
echo ">>> Setting up test disk..."
./tools/testing/setup-test-disk.sh

echo ">>> Build Complete! Artifacts ready for testing."
