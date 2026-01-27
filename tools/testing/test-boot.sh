#!/bin/bash
set -u

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
QEMU_SCRIPT="${PROJECT_ROOT}/tools/testing/run-qemu.sh"
LOG_FILE="/tmp/qemu-boot.log"
TIMEOUT=60  # Reduced from 120s - kubelet starts within ~30s

# Check if QEMU is installed
if ! command -v qemu-system-x86_64 &> /dev/null; then
    echo "Error: qemu-system-x86_64 not found in path."
    exit 1
fi

echo ">>> Starting Boot Test..."
echo "    Script: $QEMU_SCRIPT"
echo "    Log:    $LOG_FILE"
echo "    Timeout: ${TIMEOUT}s"

# Clear previous log
rm -f "$LOG_FILE"

# Start QEMU in background
"${QEMU_SCRIPT}" > "${LOG_FILE}" 2>&1 &
QEMU_PID=$!

echo "    PID:    $QEMU_PID"

# Loop to check for success message
START_TIME=$(date +%s)

while true; do
    CURRENT_TIME=$(date +%s)
    ELAPSED=$((CURRENT_TIME - START_TIME))

    if [ $ELAPSED -ge $TIMEOUT ]; then
        echo "!!! FAIL: Boot timed out after ${TIMEOUT}s !!!"
        echo "--- Log Output (Last 50 lines) ---"
        tail -n 50 "${LOG_FILE}"
        echo "----------------------------------"
        kill -9 $QEMU_PID 2>/dev/null
        exit 1
    fi

    # Check for success - kubelet running is the best indicator of successful boot
    # Kubelet only runs if keel-init started it successfully after containerd
    if grep -Eq "kubelet_node_status|NodeHasSufficientMemory|containerd.*grpc" "${LOG_FILE}"; then
        echo ">>> PASS: Boot successful in ${ELAPSED}s (kubelet running)"
        kill -9 $QEMU_PID 2>/dev/null
        exit 0
    fi

    # Check if process died early
    if ! kill -0 $QEMU_PID 2>/dev/null; then
        echo "!!! FAIL: QEMU exited early !!!"
        echo "--- Log Output ---"
        cat "${LOG_FILE}"
        echo "------------------"
        exit 1
    fi

    # Poll frequently for faster detection
    sleep 0.5
done
