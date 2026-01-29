#!/bin/bash
set -u

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BUILD_DIR="${PROJECT_ROOT}/build"
LOG_FILE="/tmp/qemu-network-test.log"
TIMEOUT=90
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
    echo "Please build the project first: ./tools/build.sh"
    exit 1
fi

echo -e "${GREEN}>>> Starting Network Integration Test...${NC}"
echo "    Log:    ${LOG_FILE}"
echo "    Timeout: ${TIMEOUT}s"
echo ""

# Test counter
TESTS_PASSED=0
TESTS_FAILED=0

# Helper function to start QEMU
start_qemu() {
    local test_name="$1"
    echo -e "${YELLOW}[${test_name}] Starting QEMU...${NC}"
    
    rm -f "${LOG_FILE}"
    "${PROJECT_ROOT}/tools/testing/run-qemu.sh" > "${LOG_FILE}" 2>&1 &
    QEMU_PID=$!
    
    echo "[${test_name}] QEMU PID: ${QEMU_PID}"
    
    # Wait for boot
    local start_time=$(date +%s)
    while true; do
        local current_time=$(date +%s)
        local elapsed=$((current_time - start_time))
        
        if [ $elapsed -ge $TIMEOUT ]; then
            echo -e "${RED}[${test_name}] FAIL: Boot timeout${NC}"
            kill -9 $QEMU_PID 2>/dev/null
            return 1
        fi
        
        # Check for successful boot (keel-agent running)
        if grep -q "keel-agent.*Starting gRPC server" "${LOG_FILE}" 2>/dev/null; then
            echo -e "${GREEN}[${test_name}] Boot successful in ${elapsed}s${NC}"
            sleep 2  # Give services time to stabilize
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
    kill -9 $QEMU_PID 2>/dev/null || true
    sleep 1
}

# Helper function to run osctl command
run_osctl() {
    local cmd="$*"
    echo "    $ osctl ${cmd}"
    "${OSCTL}" --endpoint localhost:50052 $cmd
}

# Helper function to check network config
check_network_config() {
    local test_name="$1"
    local expected_pattern="$2"
    
    echo "[${test_name}] Checking network configuration..."
    local output=$(run_osctl network config show 2>&1)
    
    if echo "$output" | grep -q "$expected_pattern"; then
        echo -e "${GREEN}[${test_name}] ✓ Configuration verified${NC}"
        return 0
    else
        echo -e "${RED}[${test_name}] ✗ Configuration check failed${NC}"
        echo "Expected pattern: ${expected_pattern}"
        echo "Actual output:"
        echo "$output"
        return 1
    fi
}

# Helper function to check network status
check_network_status() {
    local test_name="$1"
    local interface="$2"
    
    echo "[${test_name}] Checking network status for ${interface}..."
    local output=$(run_osctl network status 2>&1)
    
    if echo "$output" | grep -q "$interface"; then
        echo -e "${GREEN}[${test_name}] ✓ Interface ${interface} found${NC}"
        echo "$output" | grep -A 5 "$interface"
        return 0
    else
        echo -e "${RED}[${test_name}] ✗ Interface ${interface} not found${NC}"
        return 1
    fi
}

# =============================================================================
# TEST 1: DHCP Fallback (Default Behavior)
# =============================================================================
echo ""
echo -e "${YELLOW}========================================${NC}"
echo -e "${YELLOW}TEST 1: DHCP Fallback${NC}"
echo -e "${YELLOW}========================================${NC}"

if start_qemu "TEST1"; then
    if check_network_status "TEST1" "eth0"; then
        echo -e "${GREEN}TEST 1: PASS${NC}"
        ((TESTS_PASSED++))
    else
        echo -e "${RED}TEST 1: FAIL${NC}"
        ((TESTS_FAILED++))
    fi
    stop_qemu "TEST1"
else
    echo -e "${RED}TEST 1: FAIL (boot failed)${NC}"
    ((TESTS_FAILED++))
fi

# =============================================================================
# TEST 2: IPv4 Static Configuration
# =============================================================================
echo ""
echo -e "${YELLOW}========================================${NC}"
echo -e "${YELLOW}TEST 2: IPv4 Static Configuration${NC}"
echo -e "${YELLOW}========================================${NC}"

if start_qemu "TEST2"; then
    echo "[TEST2] Configuring static IPv4..."
    if run_osctl network config set \
        --interface eth0 \
        --ip 192.168.100.10/24 \
        --gateway 192.168.100.1 \
        --mtu 1400; then
        
        if check_network_config "TEST2" "192.168.100.10/24"; then
            echo -e "${GREEN}TEST 2: PASS${NC}"
            ((TESTS_PASSED++))
        else
            echo -e "${RED}TEST 2: FAIL (config verification)${NC}"
            ((TESTS_FAILED++))
        fi
    else
        echo -e "${RED}TEST 2: FAIL (config set)${NC}"
        ((TESTS_FAILED++))
    fi
    stop_qemu "TEST2"
else
    echo -e "${RED}TEST 2: FAIL (boot failed)${NC}"
    ((TESTS_FAILED++))
fi

# =============================================================================
# TEST 3: IPv6-Only Configuration
# =============================================================================
echo ""
echo -e "${YELLOW}========================================${NC}"
echo -e "${YELLOW}TEST 3: IPv6-Only Configuration${NC}"
echo -e "${YELLOW}========================================${NC}"

if start_qemu "TEST3"; then
    echo "[TEST3] Configuring IPv6-only..."
    if run_osctl network config set \
        --interface eth0 \
        --ipv6 2001:db8::10/64 \
        --ipv6-gateway 2001:db8::1; then
        
        if check_network_config "TEST3" "2001:db8::10/64"; then
            echo -e "${GREEN}TEST 3: PASS${NC}"
            ((TESTS_PASSED++))
        else
            echo -e "${RED}TEST 3: FAIL (config verification)${NC}"
            ((TESTS_FAILED++))
        fi
    else
        echo -e "${RED}TEST 3: FAIL (config set)${NC}"
        ((TESTS_FAILED++))
    fi
    stop_qemu "TEST3"
else
    echo -e "${RED}TEST 3: FAIL (boot failed)${NC}"
    ((TESTS_FAILED++))
fi

# =============================================================================
# TEST 4: Dual-Stack Configuration
# =============================================================================
echo ""
echo -e "${YELLOW}========================================${NC}"
echo -e "${YELLOW}TEST 4: Dual-Stack (IPv4 + IPv6)${NC}"
echo -e "${YELLOW}========================================${NC}"

if start_qemu "TEST4"; then
    echo "[TEST4] Configuring dual-stack..."
    if run_osctl network config set \
        --interface eth0 \
        --ip 192.168.100.20/24 \
        --gateway 192.168.100.1 \
        --ipv6 2001:db8::20/64 \
        --ipv6 fd00::20/64 \
        --ipv6-gateway 2001:db8::1; then
        
        # Check both IPv4 and IPv6
        local ipv4_ok=false
        local ipv6_ok=false
        
        if check_network_config "TEST4" "192.168.100.20/24"; then
            ipv4_ok=true
        fi
        
        if check_network_config "TEST4" "2001:db8::20/64"; then
            ipv6_ok=true
        fi
        
        if [ "$ipv4_ok" = true ] && [ "$ipv6_ok" = true ]; then
            echo -e "${GREEN}TEST 4: PASS${NC}"
            ((TESTS_PASSED++))
        else
            echo -e "${RED}TEST 4: FAIL (config verification)${NC}"
            ((TESTS_FAILED++))
        fi
    else
        echo -e "${RED}TEST 4: FAIL (config set)${NC}"
        ((TESTS_FAILED++))
    fi
    stop_qemu "TEST4"
else
    echo -e "${RED}TEST 4: FAIL (boot failed)${NC}"
    ((TESTS_FAILED++))
fi

# =============================================================================
# TEST 5: DNS Configuration
# =============================================================================
echo ""
echo -e "${YELLOW}========================================${NC}"
echo -e "${YELLOW}TEST 5: DNS Configuration${NC}"
echo -e "${YELLOW}========================================${NC}"

if start_qemu "TEST5"; then
    echo "[TEST5] Configuring DNS..."
    if run_osctl network dns set \
        --nameserver 8.8.8.8 \
        --nameserver 2001:4860:4860::8888 \
        --search example.com; then
        
        if check_network_config "TEST5" "8.8.8.8"; then
            echo -e "${GREEN}TEST 5: PASS${NC}"
            ((TESTS_PASSED++))
        else
            echo -e "${RED}TEST 5: FAIL (config verification)${NC}"
            ((TESTS_FAILED++))
        fi
    else
        echo -e "${RED}TEST 5: FAIL (config set)${NC}"
        ((TESTS_FAILED++))
    fi
    stop_qemu "TEST5"
else
    echo -e "${RED}TEST 5: FAIL (boot failed)${NC}"
    ((TESTS_FAILED++))
fi

# =============================================================================
# TEST 6: Configuration Persistence (Reboot Test)
# =============================================================================
echo ""
echo -e "${YELLOW}========================================${NC}"
echo -e "${YELLOW}TEST 6: Configuration Persistence${NC}"
echo -e "${YELLOW}========================================${NC}"

echo "[TEST6] This test requires manual verification:"
echo "  1. Configure network with osctl"
echo "  2. Reboot the system"
echo "  3. Verify configuration is applied at boot"
echo "  4. Check network status shows configured IPs"
echo ""
echo -e "${YELLOW}TEST 6: SKIPPED (manual test)${NC}"

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
    echo -e "${GREEN}>>> ALL TESTS PASSED! ✓${NC}"
    exit 0
else
    echo -e "${RED}>>> SOME TESTS FAILED ✗${NC}"
    exit 1
fi
