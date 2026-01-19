#!/bin/bash
set -u

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
LOG_FILE="${PROJECT_ROOT}/build/qemu-integration.log"
TIMEOUT=60

echo ">>> Starting Integration Test: Service Verification..."

# Start QEMU in background with output redirected to log
echo "Booting QEMU (this may take a few seconds)..."
rm -f "${LOG_FILE}"
nohup "${PROJECT_ROOT}/tools/testing/run-qemu.sh" > "${LOG_FILE}" 2>&1 &
QEMU_PID=$!

echo "QEMU PID: ${QEMU_PID}"
echo "Waiting for services to start..."

# Wait loop
START_TIME=$(date +%s)
FOUND_CONTAINERD=0
FOUND_KUBELET=0

while true; do
    CURRENT_TIME=$(date +%s)
    ELAPSED=$((CURRENT_TIME - START_TIME))
    
    if [ $ELAPSED -gt $TIMEOUT ]; then
        echo "!!! FAIL: Timeout waiting for services after ${TIMEOUT}s !!!"
        echo "--- Log Output (Last 50 lines) ---"
        tail -n 50 "${LOG_FILE}"
        echo "----------------------------------"
        kill -9 $QEMU_PID 2>/dev/null
        exit 1
    fi

    # Check for containerd - it logs gRPC messages when ready
    if grep -Eq "containerd.*grpc|containerd.*started" "${LOG_FILE}" 2>/dev/null; then
        FOUND_CONTAINERD=1
    fi
    
    # Check for kubelet - it logs node status messages when running
    if grep -Eq "kubelet_node_status|NodeHasSufficientMemory" "${LOG_FILE}" 2>/dev/null; then
        FOUND_KUBELET=1
    fi

    if [ $FOUND_CONTAINERD -eq 1 ] && [ $FOUND_KUBELET -eq 1 ]; then
        echo ">>> PASS: All core services verified in ${ELAPSED}s!"
        echo "  - containerd: Running"
        echo "  - kubelet: Running"
        kill -9 $QEMU_PID 2>/dev/null
        exit 0
    fi
    
    # Check if QEMU died early
    if ! kill -0 $QEMU_PID 2>/dev/null; then
        echo "!!! FAIL: QEMU exited early !!!"
        echo "--- Log Output ---"
        cat "${LOG_FILE}"
        echo "------------------"
        exit 1
    fi
    
    sleep 0.5
done
