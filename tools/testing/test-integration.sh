#!/bin/bash
set -e

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
LOG_FILE="${PROJECT_ROOT}/build/qemu.log"
TIMEOUT=60

echo ">>> Starting Integration Test: Kubelet Spawn..."

# Start QEMU in background with output redirected to log
echo "Booting QEMU (this may take a few seconds)..."
# We use a subshell to redirect all output easier, and detach
nohup "${PROJECT_ROOT}/tools/testing/run-qemu.sh" > "${LOG_FILE}" 2>&1 &
QEMU_PID=$!

echo "QEMU PID: ${QEMU_PID}"
echo "Waiting for services to start..."

# Wait loop
START_TIME=$(date +%s)
FOUND_CONTAINERD=0
FOUND_AGENT=0
FOUND_KUBELET=0

while true; do
    CURRENT_TIME=$(date +%s)
    ELAPSED=$((CURRENT_TIME - START_TIME))
    
    if [ $ELAPSED -gt $TIMEOUT ]; then
        echo "TIMEOUT waiting for services."
        kill $QEMU_PID
        exit 1
    fi

    if grep -q "containerd spawned with PID" "${LOG_FILE}"; then
        FOUND_CONTAINERD=1
    fi
    if grep -q "Matic Agent spawned with PID" "${LOG_FILE}"; then
        FOUND_AGENT=1
    fi
    if grep -q "kubelet spawned with PID" "${LOG_FILE}"; then
        FOUND_KUBELET=1
    fi

    if [ $FOUND_CONTAINERD -eq 1 ] && [ $FOUND_AGENT -eq 1 ] && [ $FOUND_KUBELET -eq 1 ]; then
        echo "SUCCESS: All services spawned!"
        echo "--------------------------------"
        grep "spawned with PID" "${LOG_FILE}"
        echo "--------------------------------"
        break
    fi
    
    sleep 2
done

# Cleanup
echo "Killing QEMU..."
kill $QEMU_PID
wait $QEMU_PID 2>/dev/null || true
echo "Test Passed."
exit 0
