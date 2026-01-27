#!/bin/bash
set -e

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BUILD_DIR="${PROJECT_ROOT}/build"
LOG_FILE="${BUILD_DIR}/qemu-update.log"
UPDATE_SERVER_PORT=8080
TIMEOUT=180

echo ">>> Starting Phase 5 Update Test..."

# 1. Setup Disk and Initramfs
docker run --rm \
    -v "${PROJECT_ROOT}:/keelos" \
    -v "keelos-cargo-cache:/root/.cargo/registry" \
    -v "keelos-target-cache:/keelos/target" \
    keelos-builder \
    /bin/bash -c "cargo build --release --target x86_64-unknown-linux-musl --package keel-init && cargo build --release --target x86_64-unknown-linux-musl --package keel-agent && cargo build --release --target x86_64-unknown-linux-musl --package osctl && chmod +x ./tools/testing/setup-test-disk.sh && ./tools/testing/setup-test-disk.sh && ./tools/builder/initramfs-build.sh"

# 2. Prepare Dummy Update Image
echo "Creating dummy update image..."
echo "NEW_VERSION_IMAGE" > "${BUILD_DIR}/update.squashfs"

# 3. Start Local HTTP Server
echo "Starting local update server on port ${UPDATE_SERVER_PORT}..."
# Use python3 to serve the build directory
cd "${BUILD_DIR}"
python3 -m http.server "${UPDATE_SERVER_PORT}" &
SERVER_PID=$!
cd -

# Ensure server is killed on exit
trap "kill $SERVER_PID || true" EXIT

# 4. Boot KeelOS in background
echo "Booting KeelOS in TEST MODE..."
export EXTRA_APPEND="test_update=1"
nohup "${PROJECT_ROOT}/tools/testing/run-qemu.sh" > "${LOG_FILE}" 2>&1 &
QEMU_PID=$!
trap "kill $QEMU_PID || true; kill $SERVER_PID || true" EXIT

echo "Waiting for Matic Agent and in-VM test..."
# Wait longer for the 15s delay in keel-init
START_TIME=$(date +%s)
while true; do
    if grep -q "in-VM update test finished" "${LOG_FILE}"; then
        echo "In-VM test finished!"
        break
    fi
    ELAPSED=$(($(date +%s) - START_TIME))
    if [ $ELAPSED -gt 120 ]; then
        echo "TIMEOUT: In-VM test failed to finish."
        tail -n 20 "${LOG_FILE}"
        exit 1
    fi
    sleep 5
done

# 6. Verify Logs
echo "Verifying agent logs..."
if grep -q "Identifying target partition..." "${LOG_FILE}" && \
   grep -q "Target partition identified: /dev/sda3" "${LOG_FILE}" && \
   grep -q "Flashing http://10.0.2.2:8080/update.squashfs to /dev/sda3..." "${LOG_FILE}" && \
   grep -q "Update installed successfully" "${LOG_FILE}"; then
    echo "SUCCESS: Update flow verified!"
else
    echo "FAILURE: Update flow logs not found or incorrect."
    echo "--- Log Output ---"
    cat "${LOG_FILE}"
    exit 1
fi

echo "Test phase 5 complete."
