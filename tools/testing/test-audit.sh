#!/bin/bash
set -u

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BUILD_DIR="${PROJECT_ROOT}/build"
LOG_FILE="/tmp/qemu-audit-test.log"
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

echo -e "${GREEN}>>> Starting Audit Logging E2E Test...${NC}"
echo "    Log:    ${LOG_FILE}"
echo "    Timeout: ${TIMEOUT}s"
echo ""

# Test counter
TESTS_PASSED=0
TESTS_FAILED=0

# Each test uses a unique port to avoid stale port state from previous QEMU instances
NEXT_PORT=50070

# Cleanup trap to remove temporary disk images on exit
cleanup() {
    rm -f "${BUILD_DIR}"/sda-audit-*.img 2>/dev/null || true
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
    CURRENT_DISK="${BUILD_DIR}/sda-audit-${test_name}.img"
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
# TEST 1: Audit Log Created After API Call
# =============================================================================
echo ""
echo -e "${YELLOW}========================================${NC}"
echo -e "${YELLOW}TEST 1: Audit Log Created After API Call${NC}"
echo -e "${YELLOW}========================================${NC}"

if start_qemu "TEST1"; then
    echo "[TEST1] Making a GetStatus API call..."
    output=$(run_osctl_capture status)
    exit_code=$?

    if [ $exit_code -eq 0 ]; then
        echo -e "${GREEN}[TEST1] ✓ GetStatus call succeeded${NC}"
        echo "$output"

        # Check the QEMU log for audit tracing output
        # The audit layer emits tracing events containing "audit" with method info
        echo "[TEST1] Checking for audit log traces..."
        sleep 2  # Allow time for log flush

        if grep -q "audit" "${LOG_FILE}" && grep -q "GetStatus" "${LOG_FILE}"; then
            echo -e "${GREEN}[TEST1] ✓ Audit trace found in agent logs for GetStatus${NC}"
            echo -e "${GREEN}TEST 1: PASS${NC}"
            ((TESTS_PASSED++))
        else
            echo -e "${RED}[TEST1] ✗ No audit trace found in agent logs${NC}"
            echo "[TEST1] Relevant log lines:"
            grep -i "audit\|GetStatus" "${LOG_FILE}" | tail -10
            echo -e "${RED}TEST 1: FAIL${NC}"
            ((TESTS_FAILED++))
        fi
    else
        echo -e "${RED}[TEST1] ✗ GetStatus call failed${NC}"
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
# TEST 2: Multiple Operations Produce Multiple Audit Entries
# =============================================================================
echo ""
echo -e "${YELLOW}========================================${NC}"
echo -e "${YELLOW}TEST 2: Multiple Operations Audited${NC}"
echo -e "${YELLOW}========================================${NC}"

if start_qemu "TEST2"; then
    echo "[TEST2] Making multiple API calls..."

    # Call 1: GetStatus
    output1=$(run_osctl_capture status)
    echo "[TEST2] GetStatus: exit=$?"

    # Call 2: Health
    output2=$(run_osctl_capture health)
    echo "[TEST2] Health: exit=$?"

    # Call 3: Debug status (no side effects)
    output3=$(run_osctl_capture diag debug-status)
    echo "[TEST2] DebugStatus: exit=$?"

    sleep 2  # Allow time for log flush

    # Count audit trace lines in the QEMU log
    audit_count=$(grep -c "audit" "${LOG_FILE}" 2>/dev/null || echo "0")
    echo "[TEST2] Audit trace count: ${audit_count}"

    if [ "$audit_count" -ge 3 ]; then
        echo -e "${GREEN}[TEST2] ✓ Multiple audit entries found (${audit_count} traces)${NC}"
        echo -e "${GREEN}TEST 2: PASS${NC}"
        ((TESTS_PASSED++))
    else
        echo -e "${RED}[TEST2] ✗ Expected at least 3 audit entries, found ${audit_count}${NC}"
        echo "[TEST2] Relevant log lines:"
        grep -i "audit" "${LOG_FILE}" | tail -10
        echo -e "${RED}TEST 2: FAIL${NC}"
        ((TESTS_FAILED++))
    fi
    stop_qemu "TEST2"
else
    echo -e "${RED}TEST 2: FAIL (boot failed)${NC}"
    ((TESTS_FAILED++))
fi

# =============================================================================
# TEST 3: Audit Entry Contains Method Name
# =============================================================================
echo ""
echo -e "${YELLOW}========================================${NC}"
echo -e "${YELLOW}TEST 3: Audit Entry Contains Method Name${NC}"
echo -e "${YELLOW}========================================${NC}"

if start_qemu "TEST3"; then
    echo "[TEST3] Making a Health API call..."
    output=$(run_osctl_capture health)

    sleep 2  # Allow time for log flush

    # Verify audit trace contains the gRPC method path
    if grep -q "audit" "${LOG_FILE}" && grep -q "GetHealth" "${LOG_FILE}"; then
        echo -e "${GREEN}[TEST3] ✓ Audit trace includes GetHealth method name${NC}"
        echo "[TEST3] Matching log lines:"
        grep "GetHealth" "${LOG_FILE}" | tail -5
        echo -e "${GREEN}TEST 3: PASS${NC}"
        ((TESTS_PASSED++))
    else
        echo -e "${RED}[TEST3] ✗ Audit trace missing GetHealth method name${NC}"
        echo "[TEST3] Relevant log lines:"
        grep -i "audit\|health" "${LOG_FILE}" | tail -10
        echo -e "${RED}TEST 3: FAIL${NC}"
        ((TESTS_FAILED++))
    fi
    stop_qemu "TEST3"
else
    echo -e "${RED}TEST 3: FAIL (boot failed)${NC}"
    ((TESTS_FAILED++))
fi

# =============================================================================
# TEST 4: Audit Captures Diagnostic Operations
# =============================================================================
echo ""
echo -e "${YELLOW}========================================${NC}"
echo -e "${YELLOW}TEST 4: Audit Captures Diagnostic Ops${NC}"
echo -e "${YELLOW}========================================${NC}"

if start_qemu "TEST4"; then
    echo "[TEST4] Enabling debug mode..."
    output=$(run_osctl_capture diag debug --duration 300 --reason "audit e2e test")

    sleep 2  # Allow time for log flush

    # Verify audit trace captured the EnableDebugMode call
    if grep -q "audit" "${LOG_FILE}" && grep -q "EnableDebugMode" "${LOG_FILE}"; then
        echo -e "${GREEN}[TEST4] ✓ Audit trace includes EnableDebugMode${NC}"
        echo "[TEST4] Matching log lines:"
        grep "EnableDebugMode" "${LOG_FILE}" | tail -5
        echo -e "${GREEN}TEST 4: PASS${NC}"
        ((TESTS_PASSED++))
    else
        echo -e "${RED}[TEST4] ✗ Audit trace missing EnableDebugMode${NC}"
        echo "[TEST4] Relevant log lines:"
        grep -i "audit\|debug" "${LOG_FILE}" | tail -10
        echo -e "${RED}TEST 4: FAIL${NC}"
        ((TESTS_FAILED++))
    fi
    stop_qemu "TEST4"
else
    echo -e "${RED}TEST 4: FAIL (boot failed)${NC}"
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
    echo -e "${GREEN}>>> ALL AUDIT LOGGING TESTS PASSED! ✓${NC}"
    exit 0
else
    echo -e "${RED}>>> SOME AUDIT LOGGING TESTS FAILED ✗${NC}"
    exit 1
fi
