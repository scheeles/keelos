#!/bin/bash
set -e

# This script is intended to be run INSIDE the builder container.
# It builds the OS components and runs all tests.

echo ">>> Starting CI Build Process..."

# =============================================================================
# BUILD PHASE
# =============================================================================

# 1. Build Rust Binaries (Init, Agent, API)
echo ">>> Building Rust components (musl)..."
cargo build --release --target x86_64-unknown-linux-musl --workspace

# 2. Run Unit Tests
echo ">>> Running Rust tests..."
cargo test --workspace

# 3. Build Kernel
echo ">>> Building Kernel..."
./tools/builder/kernel-build.sh

# 4. Build Initramfs
echo ">>> Building Initramfs..."
./tools/builder/initramfs-build.sh

# 5. Build ISO
echo ">>> Building ISO..."
./tools/builder/iso-build.sh

# =============================================================================
# TEST PHASE
# =============================================================================

# 6. Verify Build Artifacts
echo ">>> Verifying build artifacts..."
./tools/testing/verify-artifacts.sh

# 7. Setup Test Disk (required for QEMU tests)
echo ">>> Setting up test disk..."
./tools/testing/setup-test-disk.sh

# 8. Boot Test - Verify system boots and kubelet starts
echo ">>> Running QEMU Boot Test..."
./tools/testing/test-boot.sh

# 9. Integration Test - Verify all services spawn correctly
echo ">>> Running Integration Test..."
./tools/testing/test-integration.sh

# Note: test-update-flow.sh is skipped in CI as it requires:
# - Docker-in-docker (not available in standard CI)
# - Python HTTP server setup
# It should be run manually or in a dedicated e2e test environment.

echo ">>> CI Build Complete! All tests passed."
