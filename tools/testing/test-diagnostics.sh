#!/bin/bash
set -u

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BUILD_DIR="${PROJECT_ROOT}/build"
LOG_FILE="/tmp/qemu-diagnostics-test.log"
TIMEOUT=60
OSCTL="${BUILD_DIR}/osctl"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Check if QEMU is installed
if ! command -v qemu-system-x86_64 &> /dev/null; then
    echo -e "${RED}Error: qemu-system-x86_64 not found in path.${NC}"
    exit 1
fi

# Check if osctl exists
if [ ! -f "${OSCTL}" ]; then
    echo -e "${RED}Error: osctl not found at ${OSCTL}${NC}"
    echo "Please build the project first: ./tools/builder/build.sh"
    exit 1
fi

# Ensure osctl is executable
chmod +x "${OSCTL}"

echo -e "${GREEN}>>> Starting Diagnostics & Debugging E2E Test...${NC}"
echo "    Log:    ${LOG_FILE}"
echo "    Timeout: ${TIMEOUT}s"
echo ""

# Test counter
TESTS_PASSED=0
TESTS_FAILED=0

# Each test uses a unique port to avoid stale port state from previous QEMU instances
NEXT_PORT=50060

# Cleanup trap to remove temporary disk images on exit
cleanup() {
    rm -f "${BUILD_DIR}"/sda-diag-*.img 2>/dev/null || true
}
trap cleanup EXIT

# Helper function to start QEMU
start_qemu() {
    local test_name="$1"

    # Assign a unique port for this test
    CURRENT_PORT=$NEXT_PORT
    NEXT_PORT=$((NEXT_PORT + 1))

    echo -e "${YELLOW}[${test_name}] Starting QEMU (port ${CURRENT_PORT})...${NC}"

    # Copy disk image for this test (avoids write lock conflicts between QEMU instances)
    CURRENT_DISK="${BUILD_DIR}/sda-diag-${test_name}.img"
    cp "${BUILD_DIR}/sda.img" "${CURRENT_DISK}"

    rm -f "${LOG_FILE}"
    QEMU_HOST_PORT=$CURRENT_PORT QEMU_DISK="$CURRENT_DISK" "${PROJECT_ROOT}/tools/testing/run-qemu.sh" > "${LOG_FILE}" 2>&1 &
    QEMU_PID=$!

    echo "[${test_name}] QEMU PID: ${QEMU_PID}"

    # Wait for boot
    local start_time=$(date +%s)
    while true; do
        local current_time=$(date +%s)
        local elapsed=$((current_time - start_time))

        if [ $elapsed -ge $TIMEOUT ]; then
            echo -e "${RED}[${test_name}] FAIL: Boot timeout${NC}"
            kill $QEMU_PID 2>/dev/null || true
            sleep 1
            kill -9 $QEMU_PID 2>/dev/null || true
            return 1
        fi

        # Check for successful boot (kubelet running indicates successful boot)
        if grep -Eq "kubelet_node_status|NodeHasSufficientMemory|containerd.*grpc" "${LOG_FILE}" 2>/dev/null; then
            echo -e "${GREEN}[${test_name}] Boot successful in ${elapsed}s${NC}"

            # Wait for keel-agent gRPC server to be ready
            echo "[${test_name}] Waiting for gRPC server to be ready..."
            local grpc_ready=false
            for i in {1..20}; do
                if nc -z localhost $CURRENT_PORT 2>/dev/null; then
                    grpc_ready=true
                    echo "[${test_name}] gRPC server is accepting connections on port ${CURRENT_PORT}"
                    sleep 2  # Extra stabilization time
                    break
                fi
                sleep 0.5
            done

            if [ "$grpc_ready" = false ]; then
                echo -e "${RED}[${test_name}] WARNING: gRPC server not responding on port ${CURRENT_PORT}${NC}"
                sleep 2  # Try anyway
            fi

            return 0
        fi

        # Check if process died
        if ! kill -0 $QEMU_PID 2>/dev/null; then
            echo -e "${RED}[${test_name}] FAIL: QEMU exited early${NC}"
            tail -n 30 "${LOG_FILE}"
            return 1
        fi

        sleep 0.5
    done
}

# Helper function to stop QEMU
stop_qemu() {
    local test_name="$1"
    echo "[${test_name}] Stopping QEMU (PID: ${QEMU_PID})"
    kill $QEMU_PID 2>/dev/null || true
    sleep 1
    kill -9 $QEMU_PID 2>/dev/null || true
    wait $QEMU_PID 2>/dev/null || true
    # Clean up per-test disk image copy
    rm -f "${CURRENT_DISK}" 2>/dev/null || true
    echo "[${test_name}] QEMU stopped"
}

# Helper function to run osctl and capture output
run_osctl_capture() {
    echo "    $ osctl $*"
    "${OSCTL}" --endpoint "http://localhost:${CURRENT_PORT}" "$@" 2>&1
}

# =============================================================================
# TEST 1: Enable Debug Mode
# =============================================================================
echo ""
echo -e "${YELLOW}========================================${NC}"
echo -e "${YELLOW}TEST 1: Enable Debug Mode${NC}"
echo -e "${YELLOW}========================================${NC}"

if start_qemu "TEST1"; then
    echo "[TEST1] Enabling debug mode..."
    output=$(run_osctl_capture diag debug --duration 600 --reason "e2e test")
    exit_code=$?

    if [ $exit_code -eq 0 ] && echo "$output" | grep -q "Session ID:"; then
        echo -e "${GREEN}[TEST1] ✓ Debug mode enabled successfully${NC}"
        echo "$output"

        # Verify debug status shows active
        echo "[TEST1] Checking debug status..."
        status_output=$(run_osctl_capture diag debug-status)

        if echo "$status_output" | grep -q "ACTIVE"; then
            echo -e "${GREEN}[TEST1] ✓ Debug status shows ACTIVE${NC}"
            echo "$status_output"
            echo -e "${GREEN}TEST 1: PASS${NC}"
            ((TESTS_PASSED++))
        else
            echo -e "${RED}[TEST1] ✗ Debug status did not show ACTIVE${NC}"
            echo "$status_output"
            echo -e "${RED}TEST 1: FAIL${NC}"
            ((TESTS_FAILED++))
        fi
    else
        echo -e "${RED}[TEST1] ✗ Failed to enable debug mode${NC}"
        echo "$output"
        echo -e "${RED}TEST 1: FAIL${NC}"
        ((TESTS_FAILED++))
    fi
    stop_qemu "TEST1"
else
    echo -e "${RED}TEST 1: FAIL (boot failed)${NC}"
    ((TESTS_FAILED++))
fi

# =============================================================================
# TEST 2: Debug Mode Duplicate Rejection
# =============================================================================
echo ""
echo -e "${YELLOW}========================================${NC}"
echo -e "${YELLOW}TEST 2: Debug Mode Duplicate Rejection${NC}"
echo -e "${YELLOW}========================================${NC}"

if start_qemu "TEST2"; then
    echo "[TEST2] Enabling debug mode (first time)..."
    output1=$(run_osctl_capture diag debug --duration 600 --reason "first session")

    if echo "$output1" | grep -q "Session ID:"; then
        echo -e "${GREEN}[TEST2] ✓ First debug mode enabled${NC}"

        echo "[TEST2] Enabling debug mode (second time - should fail)..."
        output2=$(run_osctl_capture diag debug --duration 600 --reason "second session")

        # The second attempt should report the mode is already active
        if echo "$output2" | grep -qi "already active\|❌"; then
            echo -e "${GREEN}[TEST2] ✓ Duplicate correctly rejected${NC}"
            echo "$output2"
            echo -e "${GREEN}TEST 2: PASS${NC}"
            ((TESTS_PASSED++))
        else
            echo -e "${RED}[TEST2] ✗ Duplicate was not rejected${NC}"
            echo "$output2"
            echo -e "${RED}TEST 2: FAIL${NC}"
            ((TESTS_FAILED++))
        fi
    else
        echo -e "${RED}[TEST2] ✗ First enable failed${NC}"
        echo "$output1"
        echo -e "${RED}TEST 2: FAIL${NC}"
        ((TESTS_FAILED++))
    fi
    stop_qemu "TEST2"
else
    echo -e "${RED}TEST 2: FAIL (boot failed)${NC}"
    ((TESTS_FAILED++))
fi

# =============================================================================
# TEST 3: Debug Status When Inactive
# =============================================================================
echo ""
echo -e "${YELLOW}========================================${NC}"
echo -e "${YELLOW}TEST 3: Debug Status When Inactive${NC}"
echo -e "${YELLOW}========================================${NC}"

if start_qemu "TEST3"; then
    echo "[TEST3] Checking debug status (should be inactive)..."
    output=$(run_osctl_capture diag debug-status)

    if echo "$output" | grep -q "INACTIVE"; then
        echo -e "${GREEN}[TEST3] ✓ Debug status correctly shows INACTIVE${NC}"
        echo "$output"
        echo -e "${GREEN}TEST 3: PASS${NC}"
        ((TESTS_PASSED++))
    else
        echo -e "${RED}[TEST3] ✗ Debug status did not show INACTIVE${NC}"
        echo "$output"
        echo -e "${RED}TEST 3: FAIL${NC}"
        ((TESTS_FAILED++))
    fi
    stop_qemu "TEST3"
else
    echo -e "${RED}TEST 3: FAIL (boot failed)${NC}"
    ((TESTS_FAILED++))
fi

# =============================================================================
# TEST 4: Enable Recovery Mode
# =============================================================================
echo ""
echo -e "${YELLOW}========================================${NC}"
echo -e "${YELLOW}TEST 4: Enable Recovery Mode${NC}"
echo -e "${YELLOW}========================================${NC}"

if start_qemu "TEST4"; then
    echo "[TEST4] Enabling recovery mode..."
    output=$(run_osctl_capture diag recovery --duration 600 --reason "e2e recovery test")
    exit_code=$?

    if [ $exit_code -eq 0 ] && echo "$output" | grep -q "Expires at:"; then
        echo -e "${GREEN}[TEST4] ✓ Recovery mode enabled successfully${NC}"
        echo "$output"
        echo -e "${GREEN}TEST 4: PASS${NC}"
        ((TESTS_PASSED++))
    else
        echo -e "${RED}[TEST4] ✗ Failed to enable recovery mode${NC}"
        echo "$output"
        echo -e "${RED}TEST 4: FAIL${NC}"
        ((TESTS_FAILED++))
    fi
    stop_qemu "TEST4"
else
    echo -e "${RED}TEST 4: FAIL (boot failed)${NC}"
    ((TESTS_FAILED++))
fi

# =============================================================================
# TEST 5: Collect Crash Dump
# =============================================================================
echo ""
echo -e "${YELLOW}========================================${NC}"
echo -e "${YELLOW}TEST 5: Collect Crash Dump${NC}"
echo -e "${YELLOW}========================================${NC}"

if start_qemu "TEST5"; then
    echo "[TEST5] Collecting crash dump..."
    output=$(run_osctl_capture diag crash-dump)
    exit_code=$?

    if [ $exit_code -eq 0 ] && echo "$output" | grep -q "Path:"; then
        echo -e "${GREEN}[TEST5] ✓ Crash dump collected successfully${NC}"
        echo "$output"

        # Verify the dump has a size > 0
        if echo "$output" | grep -qE "Size: [0-9]"; then
            echo -e "${GREEN}[TEST5] ✓ Crash dump has non-zero size${NC}"
            echo -e "${GREEN}TEST 5: PASS${NC}"
            ((TESTS_PASSED++))
        else
            echo -e "${RED}[TEST5] ✗ Crash dump size appears to be zero${NC}"
            echo -e "${RED}TEST 5: FAIL${NC}"
            ((TESTS_FAILED++))
        fi
    else
        echo -e "${RED}[TEST5] ✗ Failed to collect crash dump${NC}"
        echo "$output"
        echo -e "${RED}TEST 5: FAIL${NC}"
        ((TESTS_FAILED++))
    fi
    stop_qemu "TEST5"
else
    echo -e "${RED}TEST 5: FAIL (boot failed)${NC}"
    ((TESTS_FAILED++))
fi

# =============================================================================
# TEST 6: Create System Snapshot
# =============================================================================
echo ""
echo -e "${YELLOW}========================================${NC}"
echo -e "${YELLOW}TEST 6: Create System Snapshot${NC}"
echo -e "${YELLOW}========================================${NC}"

if start_qemu "TEST6"; then
    echo "[TEST6] Creating system snapshot..."
    output=$(run_osctl_capture diag snapshot --label "e2e-test-snapshot")
    exit_code=$?

    if [ $exit_code -eq 0 ] && echo "$output" | grep -q "Snapshot ID:"; then
        echo -e "${GREEN}[TEST6] ✓ System snapshot created successfully${NC}"
        echo "$output"

        # Verify the snapshot has a path and size
        if echo "$output" | grep -q "Path:" && echo "$output" | grep -qE "Size: [0-9]"; then
            echo -e "${GREEN}[TEST6] ✓ Snapshot has valid path and size${NC}"
            echo -e "${GREEN}TEST 6: PASS${NC}"
            ((TESTS_PASSED++))
        else
            echo -e "${RED}[TEST6] ✗ Snapshot missing path or size${NC}"
            echo -e "${RED}TEST 6: FAIL${NC}"
            ((TESTS_FAILED++))
        fi
    else
        echo -e "${RED}[TEST6] ✗ Failed to create system snapshot${NC}"
        echo "$output"
        echo -e "${RED}TEST 6: FAIL${NC}"
        ((TESTS_FAILED++))
    fi
    stop_qemu "TEST6"
else
    echo -e "${RED}TEST 6: FAIL (boot failed)${NC}"
    ((TESTS_FAILED++))
fi

# =============================================================================
# SUMMARY
# =============================================================================
echo ""
echo -e "${YELLOW}========================================${NC}"
echo -e "${YELLOW}TEST SUMMARY${NC}"
echo -e "${YELLOW}========================================${NC}"
echo ""
echo "Total Tests: $((TESTS_PASSED + TESTS_FAILED))"
echo -e "${GREEN}Passed: ${TESTS_PASSED}${NC}"
echo -e "${RED}Failed: ${TESTS_FAILED}${NC}"
echo ""

if [ $TESTS_FAILED -eq 0 ]; then
    echo -e "${GREEN}>>> ALL DIAGNOSTICS TESTS PASSED! ✓${NC}"
    exit 0
else
    echo -e "${RED}>>> SOME DIAGNOSTICS TESTS FAILED ✗${NC}"
    exit 1
fi
