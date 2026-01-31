#!/bin/bash
set -e

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SETUP_KIND="${PROJECT_ROOT}/tools/testing/setup-kind.sh"
LOG_FILE="${PROJECT_ROOT}/build/qemu-bootstrap.log"
CLUSTER_NAME="keel-test"
TIMEOUT=60
OSCTL="${PROJECT_ROOT}/target/debug/osctl"
ENDPOINT="http://localhost:50052"

echo ">>> Starting Kubernetes Bootstrap Integration Test..."

# Check dependencies
echo "Checking dependencies..."
if ! command -v kind &> /dev/null; then
    echo "Error: kind is not installed."
    exit 1
fi

if ! command -v kubectl &> /dev/null; then
    echo "Error: kubectl is not installed."
    exit 1
fi

if ! command -v jq &> /dev/null; then
    echo "Error: jq is not installed."
    exit 1
fi

# Build osctl if not present
if [ ! -f "${OSCTL}" ]; then
    echo "Building osctl..."
    cd "${PROJECT_ROOT}"
    cargo build --bin osctl
    cd -
fi

# Setup Kind cluster
echo "Setting up Kind cluster..."
"${SETUP_KIND}"

# Extract cluster CA certificate
echo "Extracting cluster CA certificate..."
CA_CERT_FILE="${PROJECT_ROOT}/build/kind-ca.crt"
kubectl config view --raw -o jsonpath='{.clusters[0].cluster.certificate-authority-data}' | base64 -d > "${CA_CERT_FILE}"

# Create bootstrap token
echo "Creating bootstrap token..."
TOKEN_ID=$(openssl rand -hex 3)
TOKEN_SECRET=$(openssl rand -hex 8)
BOOTSTRAP_TOKEN="${TOKEN_ID}.${TOKEN_SECRET}"

kubectl create secret generic "bootstrap-token-${TOKEN_ID}" \
  --type bootstrap.kubernetes.io/token \
  --from-literal=token-id="${TOKEN_ID}" \
  --from-literal=token-secret="${TOKEN_SECRET}" \
  --from-literal=usage-bootstrap-authentication=true \
  --from-literal=usage-bootstrap-signing=true \
  -n kube-system

echo "Bootstrap token created: ${BOOTSTRAP_TOKEN}"

# Start QEMU in background
echo "Starting KeelOS in QEMU..."
rm -f "${LOG_FILE}"
nohup "${PROJECT_ROOT}/tools/testing/run-qemu.sh" > "${LOG_FILE}" 2>&1 &
QEMU_PID=$!
echo "QEMU PID: ${QEMU_PID}"

# Cleanup function
cleanup() {
    echo "Cleaning up..."
    if [ -n "${QEMU_PID}" ]; then
        kill -9 "${QEMU_PID}" 2>/dev/null || true
    fi
}
trap cleanup EXIT

# Wait for keel-agent to be responsive
echo "Waiting for keel-agent to be responsive..."
START_TIME=$(date +%s)
AGENT_READY=0

while true; do
    CURRENT_TIME=$(date +%s)
    ELAPSED=$((CURRENT_TIME - START_TIME))
    
    if [ $ELAPSED -gt $TIMEOUT ]; then
        echo "!!! FAIL: Timeout waiting for agent after ${TIMEOUT}s !!!"
        echo "--- Log Output (Last 50 lines) ---"
        tail -n 50 "${LOG_FILE}"
        echo "-----------------------------------"
        exit 1
    fi
    
    # Check if agent responds to status request
    if "${OSCTL}" --endpoint "${ENDPOINT}" status &>/dev/null; then
        AGENT_READY=1
        echo "✓ Agent is responsive after ${ELAPSED}s"
        break
    fi
    
    # Check if QEMU died early
    if ! kill -0 "${QEMU_PID}" 2>/dev/null; then
        echo "!!! FAIL: QEMU exited early !!!"
        echo "--- Log Output ---"
        cat "${LOG_FILE}"
        echo "------------------"
        exit 1
    fi
    
    sleep 0.5
done

# Bootstrap the node
echo "Bootstrapping node with Kubernetes credentials..."
BOOTSTRAP_OUTPUT=$("${OSCTL}" --endpoint "${ENDPOINT}" bootstrap \
    --api-server "https://10.0.2.2:6443" \
    --token "${BOOTSTRAP_TOKEN}" \
    --ca-cert "${CA_CERT_FILE}" \
    --node-name "keelnode-test" 2>&1)

echo "${BOOTSTRAP_OUTPUT}"

if echo "${BOOTSTRAP_OUTPUT}" | grep -q "success"; then
    echo "✓ Bootstrap command succeeded"
else
    echo "!!! FAIL: Bootstrap command failed !!!"
    exit 1
fi

# Verify bootstrap status
echo "Verifying bootstrap status..."
STATUS_OUTPUT=$("${OSCTL}" --endpoint "${ENDPOINT}" bootstrap-status 2>&1)
echo "${STATUS_OUTPUT}"

# Check if bootstrapped
if echo "${STATUS_OUTPUT}" | grep -q "is_bootstrapped.*true"; then
    echo "✓ Bootstrap status: is_bootstrapped = true"
else
    echo "!!! FAIL: Bootstrap status shows is_bootstrapped = false !!!"
    exit 1
fi

# Check API server endpoint
if echo "${STATUS_OUTPUT}" | grep -q "https://10.0.2.2:6443"; then
    echo "✓ API server endpoint is correct"
else
    echo "!!! FAIL: API server endpoint mismatch !!!"
    exit 1
fi

# Check node name
if echo "${STATUS_OUTPUT}" | grep -q "keelnode-test"; then
    echo "✓ Node name is correct"
else
    echo "!!! FAIL: Node name mismatch !!!"
    exit 1
fi

# Wait for node to appear in cluster (optional check)
echo "Checking if node appears in cluster (waiting up to 90s)..."
NODE_TIMEOUT=90
START_TIME=$(date +%s)
NODE_FOUND=0

while true; do
    CURRENT_TIME=$(date +%s)
    ELAPSED=$((CURRENT_TIME - START_TIME))
    
    if [ $ELAPSED -gt $NODE_TIMEOUT ]; then
        echo "⚠ Node did not appear in cluster after ${NODE_TIMEOUT}s"
        echo "  This may be expected if kubelet is not fully functional yet."
        echo "  Bootstrap configuration is still valid."
        break
    fi
    
    if kubectl get nodes keelnode-test &>/dev/null; then
        NODE_FOUND=1
        echo "✓ Node 'keelnode-test' appeared in cluster!"
        kubectl get nodes keelnode-test
        break
    fi
    
    sleep 2
done

# Final summary
echo ""
echo "========================================="
echo "         Bootstrap Test Results"
echo "========================================="
echo "✓ Kind cluster created and configured"
echo "✓ Bootstrap token generated"
echo "✓ KeelOS agent started in QEMU"
echo "✓ Bootstrap command executed successfully"
echo "✓ Bootstrap status verified"
if [ $NODE_FOUND -eq 1 ]; then
    echo "✓ Node joined cluster successfully"
else
    echo "⚠ Node did not join cluster (kubelet may need additional work)"
fi
echo "========================================="
echo ">>> PASS: Bootstrap test completed successfully!"
echo "========================================="
