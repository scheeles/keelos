#!/bin/bash
set -u

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BUILD_DIR="${PROJECT_ROOT}/build"
LOG_FILE="/tmp/qemu-network-test.log"
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
    echo "Please build the project first: ./tools/build.sh"
    exit 1
fi

# Ensure osctl is executable (artifacts may lose permissions)
chmod +x "${OSCTL}"

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
    
    # Ensure port 50052 is available
    local pids=$(lsof -ti:50052 2>/dev/null)
    if [ -n "$pids" ]; then
        echo "[${test_name}] Port 50052 is in use (PIDs: $pids), killing processes..."
        echo "$pids" | xargs kill -9 2>/dev/null || true
        sleep 2
    fi
    
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
        
        # Check for successful boot (kubelet running indicates successful boot)
        # This is the same pattern used in test-boot.sh and test-integration.sh
        if grep -Eq "kubelet_node_status|NodeHasSufficientMemory|containerd.*grpc" "${LOG_FILE}" 2>/dev/null; then
            echo -e "${GREEN}[${test_name}] Boot successful in ${elapsed}s${NC}"
            
            # Wait for keel-agent gRPC server to be ready
            echo "[${test_name}] Waiting for gRPC server to be ready..."
            local grpc_ready=false
            for i in {1..20}; do
                if nc -z localhost 50052 2>/dev/null; then
                    grpc_ready=true
                    echo "[${test_name}] gRPC server is accepting connections"
                    sleep 2  # Extra stabilization time
                    break
                fi
                sleep 0.5
            done
            
            if [ "$grpc_ready" = false ]; then
                echo -e "${RED}[${test_name}] WARNING: gRPC server not responding on port 50052${NC}"
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
    kill -9 $QEMU_PID 2>/dev/null || true
    
    # Wait for the port to be released (up to 10 seconds)
    local port_free=false
    for i in {1..20}; do
        if ! lsof -ti:50052 >/dev/null 2>&1 && ! ss -tuln 2>/dev/null | grep -q ":50052 "; then
            port_free=true
            echo "[${test_name}] Port 50052 released"
            break
        fi
        sleep 0.5
    done
    
    if [ "$port_free" = false ]; then
        echo "[${test_name}] Warning: Port 50052 may still be in use"
        # Force kill any process using the port
        local pids=$(lsof -ti:50052 2>/dev/null)
        if [ -n "$pids" ]; then
            echo "$pids" | xargs kill -9 2>/dev/null || true
        fi
        sleep 1
    fi
}

# Helper function to run osctl command
run_osctl() {
    local cmd="$*"
    echo "    $ osctl ${cmd}"
    "${OSCTL}" --endpoint http://localhost:50052 $cmd
}

# Helper function to check network config
check_network_config() {
    local test_name="$1"
    local expected_pattern="$2"
    
    echo "[${test_name}] Checking network configuration..."
    local output
    output=$(run_osctl network config show 2>&1)
    
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
    
    echo "[${test_name}] Checking network status..."
    local output
    output=$(run_osctl network status 2>&1)
    local exit_code=$?
    
    if [ $exit_code -ne 0 ]; then
        echo -e "${RED}[${test_name}] ✗ osctl command failed (exit code: $exit_code)${NC}"
        echo "Output: $output"
        echo ">>> KEEL-INIT LOGS:"
        grep -E "Configured loopback|Failed to.*loopback" "${LOG_FILE}" || echo "NO LOOPBACK LOGS FOUND"
        echo ">>> QEMU/AGENT LOGS (LAST 50 LINES):"
        tail -n 50 "${LOG_FILE}"
        return 1
    fi
    
    # Check if any interface is found (excluding loopback)
    if echo "$output" | grep -qE "(eth|ens|enp|eno)[0-9]+"; then
        echo -e "${GREEN}[${test_name}] ✓ Network interface(s) found${NC}"
        echo "$output"
        return 0
    else
        echo -e "${RED}[${test_name}] ✗ No network interfaces found${NC}"
        echo "Output: $output"
        echo ">>> KEEL-INIT LOGS:"
        grep -E "Configured loopback|Failed to.*loopback" "${LOG_FILE}" || echo "NO LOOPBACK LOGS FOUND"
        echo ">>> QEMU/AGENT LOGS (LAST 50 LINES):"
        tail -n 50 "${LOG_FILE}"
        return 1
    fi
}

# =============================================================================
# TEST 1: DHCP Fallback (Default Behavior)
# =============================================================================
echo ""
echo -e "${YELLOW}========================================${NC}"
echo -e "${YELLOW}TEST 1: DHCP Fallback${NC}"
echo  -e "${YELLOW}========================================${NC}"

if start_qemu "TEST1"; then
    if check_network_status "TEST1"; then
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
        ipv4_ok=false
        ipv6_ok=false
        
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

echo -e "${YELLOW}TEST 5: SKIPPED (user requested)${NC}"
echo -e "${YELLOW}TEST 5: SKIPPED (user requested)${NC}"
# if start_qemu "TEST5"; then
#     echo "[TEST5] Configuring DNS..."
#     if run_osctl network dns set \
#         --nameserver 8.8.8.8 \
#         --nameserver 2001:4860:4860::8888 \
#         --search example.com; then
#         
#         if check_network_config "TEST5" "8.8.8.8"; then
#             echo -e "${GREEN}TEST 5: PASS${NC}"
#             ((TESTS_PASSED++))
#         else
#             echo -e "${RED}TEST 5: FAIL (config verification)${NC}"
#             ((TESTS_FAILED++))
#         fi
#     else
#         echo -e "${RED}TEST 5: FAIL (config set)${NC}"
#         ((TESTS_FAILED++))
#     fi
#     stop_qemu "TEST5"
# else
#     echo -e "${RED}TEST 5: FAIL (boot failed)${NC}"
#     ((TESTS_FAILED++))
# fi

# =============================================================================
# TEST 6: IPv6 SLAAC Auto-configuration
# =============================================================================
echo ""
echo -e "${YELLOW}========================================${NC}"
echo -e "${YELLOW}TEST 6: IPv6 SLAAC Auto-configuration${NC}"
echo -e "${YELLOW}========================================${NC}"

if start_qemu "TEST6"; then
    echo "[TEST6] Configuring IPv6 SLAAC..."
    if run_osctl network config set \
        --interface eth0 \
        --ip 192.168.100.30/24 \
        --ipv6-auto; then
        
        if check_network_config "TEST6" "ipv6_auto"; then
            echo -e "${GREEN}TEST 6: PASS${NC}"
            ((TESTS_PASSED++))
        else
            echo -e "${RED}TEST 6: FAIL (config verification)${NC}"
            ((TESTS_FAILED++))
        fi
    else
        echo -e "${RED}TEST 6: FAIL (config set)${NC}"
        ((TESTS_FAILED++))
    fi
    stop_qemu "TEST6"
else
    echo -e "${RED}TEST 6: FAIL (boot failed)${NC}"
    ((TESTS_FAILED++))
fi

# =============================================================================
# TEST 7: IPv6 Detailed Status Reporting
# =============================================================================
echo ""
echo -e "${YELLOW}========================================${NC}"
echo -e "${YELLOW}TEST 7: IPv6 Detailed Status${NC}"
echo -e "${YELLOW}========================================${NC}"

if start_qemu "TEST7"; then
    echo "[TEST7] Configuring static IPv6 and checking detailed status..."
    if run_osctl network config set \
        --interface eth0 \
        --ipv6 2001:db8::100/64; then
        
        # Reboot to apply configuration
        echo "[TEST7] Rebooting to apply configuration..."
        run_osctl reboot --reason "test7-ipv6-status" || true
        sleep 5
        
        # Restart QEMU for this test
        stop_qemu "TEST7"
        sleep 2
        
        if start_qemu "TEST7-reboot"; then
            echo "[TEST7] Checking detailed IPv6 status..."
            local status_output
            status_output=$(run_osctl network status 2>&1)
            
            # Check if IPv6 address info contains expected fields
            # Note: Actual verification would require jq to parse JSON properly
            if echo "$status_output" | grep -q "2001:db8::100"; then
                echo -e "${GREEN}TEST 7: PASS (IPv6 address found in status)${NC}"
                ((TESTS_PASSED++))
            else
                echo -e "${RED}TEST 7: FAIL (IPv6 address not in status)${NC}"
                echo "Status output: $status_output"
                ((TESTS_FAILED++))
            fi
            stop_qemu "TEST7-reboot"
        else
            echo -e "${RED}TEST 7: FAIL (reboot failed)${NC}"
            ((TESTS_FAILED++))
        fi
    else
        echo -e "${RED}TEST 7: FAIL (config set)${NC}"
        ((TESTS_FAILED++))
        stop_qemu "TEST7"
    fi
else
    echo -e "${RED}TEST 7: FAIL (boot failed)${NC}"
    ((TESTS_FAILED++))
fi

# =============================================================================
# TEST 8: VLAN with IPv6
# =============================================================================
echo ""
echo -e "${YELLOW}========================================${NC}"
echo -e "${YELLOW}TEST 8: VLAN with IPv6${NC}"
echo -e "${YELLOW}========================================${NC}"

echo -e "${YELLOW}TEST 8: SKIPPED (requires VLAN-capable QEMU network setup)${NC}"
echo "[TEST8] Note: This test requires a VLAN-aware network configuration in QEMU"
echo "[TEST8] For manual testing, use: osctl network config set --interface eth0.100 --vlan 100 --parent eth0 --ipv6 2001:db8:100::1/64"

# Uncomment when QEMU has VLAN support configured:
# if start_qemu "TEST8"; then
#     echo "[TEST8] Configuring VLAN with IPv6..."
#     if run_osctl network config set \
#         --interface eth0.100 \
#         --vlan 100 \
#         --parent eth0 \
#         --ipv6 2001:db8:100::1/64 \
#         --ipv6-auto; then
#         
#         if check_network_config "TEST8" "2001:db8:100::1/64"; then
#             echo -e "${GREEN}TEST 8: PASS${NC}"
#             ((TESTS_PASSED++))
#         else
#             echo -e "${RED}TEST 8: FAIL (config verification)${NC}"
#             ((TESTS_FAILED++))
#         fi
#     else
#         echo -e "${RED}TEST 8: FAIL (config set)${NC}"
#         ((TESTS_FAILED++))
#     fi
#     stop_qemu "TEST8"
# else
#     echo -e "${RED}TEST 8: FAIL (boot failed)${NC}"
#     ((TESTS_FAILED++))
# fi

# =============================================================================
# TEST 9: Bond with IPv6
# =============================================================================
echo ""
echo -e "${YELLOW}========================================${NC}"
echo -e "${YELLOW}TEST 9: Bond with IPv6${NC}"
echo -e "${YELLOW}========================================${NC}"

echo -e "${YELLOW}TEST 9: SKIPPED (requires multi-NIC QEMU setup)${NC}"
echo "[TEST9] Note: This test requires QEMU with multiple network interfaces"
echo "[TEST9] For manual testing, use: osctl network config set --interface bond0 --bond active-backup --slaves eth0,eth1 --ipv6 2001:db8:200::1/64"

# Uncomment when QEMU has multiple NICs configured:
# if start_qemu "TEST9"; then
#     echo "[TEST9] Configuring Bond with IPv6..."
#     if run_osctl network config set \
#         --interface bond0 \
#         --bond active-backup \
#         --slaves eth0,eth1 \
#         --ipv6 2001:db8:200::1/64; then
#         
#         if check_network_config "TEST9" "2001:db8:200::1/64"; then
#             echo -e "${GREEN}TEST 9: PASS${NC}"
#             ((TESTS_PASSED++))
#         else
#             echo -e "${RED}TEST 9: FAIL (config verification)${NC}"
#             ((TESTS_FAILED++))
#         fi
#     else
#         echo -e "${RED}TEST 9: FAIL (config set)${NC}"
#         ((TESTS_FAILED++))
#     fi
#     stop_qemu "TEST9"
# else
#     echo -e "${RED}TEST 9: FAIL (boot failed)${NC}"
#     ((TESTS_FAILED++))
# fi


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
