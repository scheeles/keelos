#!/bin/bash
set -e

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SETUP_KIND="${PROJECT_ROOT}/tools/testing/setup-kind.sh"
LOG_FILE="${PROJECT_ROOT}/build/qemu-bootstrap.log"
CLUSTER_NAME="keel-test"
TIMEOUT=60
OSCTL="${PROJECT_ROOT}/build/osctl"
ENDPOINT="http://localhost:50052"

# Use a dedicated kubeconfig to avoid macOS ~/.kube/config lock issues
export KUBECONFIG="${PROJECT_ROOT}/build/keel-test.kubeconfig"

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

# Ensure osctl is available (should be from build artifacts in CI)
if [ ! -f "${OSCTL}" ]; then
    echo "Error: osctl not found at ${OSCTL}"
    echo "In CI, it should be downloaded from build artifacts."
    echo "For local testing, run: cargo build --bin osctl && cp target/debug/osctl build/"
    exit 1
fi

# Setup Kind cluster
echo "Setting up Kind cluster..."

# Delete existing cluster for a clean start
if kind get clusters 2>/dev/null | grep -q "^${CLUSTER_NAME}$"; then
    echo "Deleting existing Kind cluster '${CLUSTER_NAME}'..."
    kind delete cluster --name "${CLUSTER_NAME}"
fi

# Recreate test disk for clean state (removes stale bootstrap credentials)
echo "Recreating test disk for clean state..."
"${PROJECT_ROOT}/tools/testing/setup-test-disk.sh" --force

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
  --from-literal=auth-extra-groups=system:bootstrappers:kubeadm:default-node-token \
  -n kube-system

echo "Bootstrap token created: ${BOOTSTRAP_TOKEN}"

# Verify bootstrap token works from host
echo "Verifying bootstrap token authentication..."
TOKEN_CHECK=$(curl -sk -H "Authorization: Bearer ${BOOTSTRAP_TOKEN}" "https://localhost:6443/api/v1/namespaces" -w "%{http_code}" -o /dev/null 2>&1)
if [ "${TOKEN_CHECK}" = "403" ]; then
    echo "✓ Bootstrap token authenticates (403 = authed but no perms on this resource, expected)"
elif [ "${TOKEN_CHECK}" = "401" ]; then
    echo "⚠ WARNING: Bootstrap token returned 401 - token auth may not be enabled on API server"
    echo "  Checking API server flags..."
    docker exec keel-test-control-plane cat /etc/kubernetes/manifests/kube-apiserver.yaml 2>/dev/null | grep -i bootstrap || echo "  No bootstrap flags found"
else
    echo "  Token check returned HTTP ${TOKEN_CHECK}"
fi

# Start QEMU in background
echo "Starting KeelOS in QEMU..."

# Pre-load container images onto the test disk's data partition
# QEMU's SLIRP networking cannot pull images from container registries,
# so we pre-populate the data partition with required images.
# keel-init will mount this partition at /data/ and import images from /data/images/
DISK_IMG="${PROJECT_ROOT}/build/sda.img"
echo ">>> Pre-loading container images onto test disk..."
LOOP_DEV=$(sudo losetup --find --show --partscan "$DISK_IMG")
DATA_PART="${LOOP_DEV}p4"
sleep 1
if [ -b "$DATA_PART" ]; then
    sudo mkfs.ext4 -q -F "$DATA_PART"
    MOUNT_DIR=$(mktemp -d)
    sudo mount "$DATA_PART" "$MOUNT_DIR"
    sudo mkdir -p "${MOUNT_DIR}/images"

    # Export kube-proxy image from Kind control plane's containerd
    # (kindnet is replaced by static bridge CNI, so only kube-proxy needed)
    IMAGES=$(docker exec keel-test-control-plane ctr --namespace k8s.io images list -q 2>/dev/null | grep -E "kube-proxy:" | grep -v sha256 || true)
    echo "  Found images to export:"
    echo "  $IMAGES"

    set +e  # Don't fail on export errors
    IMG_NUM=0
    for img in $IMAGES; do
        IMG_NUM=$((IMG_NUM + 1))
        IMG_FILE="preload-${IMG_NUM}.tar"
        echo "  Exporting $img as ${IMG_FILE}..."
        # Pipe ctr export to stdout (-) and redirect to host file
        docker exec keel-test-control-plane ctr --namespace k8s.io images export - "$img" > "/tmp/${IMG_FILE}" 2>/dev/null
        if [ $? -eq 0 ] && [ -s "/tmp/${IMG_FILE}" ]; then
            sudo cp "/tmp/${IMG_FILE}" "${MOUNT_DIR}/images/"
            rm -f "/tmp/${IMG_FILE}"
            echo "  ✓ Exported $img ($(du -h "${MOUNT_DIR}/images/${IMG_FILE}" | cut -f1))"
        else
            echo "  ⚠ Failed to export $img"
            rm -f "/tmp/${IMG_FILE}"
        fi
    done
    set -e

    sudo umount "$MOUNT_DIR"
    rmdir "$MOUNT_DIR"
else
    echo "  ⚠ Data partition not found, skipping image pre-loading"
fi
sudo losetup -d "$LOOP_DEV"

rm -f "${LOG_FILE}"
export EXTRA_APPEND="hostname=keelnode-test test_cni=1"
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

# QEMU user-mode (SLIRP) networking reaches the host at 10.0.2.2
HOST_IP="10.0.2.2"
echo "Using API server at: https://${HOST_IP}:6443"

# Bootstrap the node
echo "Bootstrapping node with Kubernetes credentials..."
BOOTSTRAP_OUTPUT=$("${OSCTL}" --endpoint "${ENDPOINT}" bootstrap \
    --api-server "https://${HOST_IP}:6443" \
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
if echo "${STATUS_OUTPUT}" | grep -q "Node is bootstrapped"; then
    echo "✓ Bootstrap status: Node is bootstrapped"
else
    echo "!!! FAIL: Bootstrap status shows node is NOT bootstrapped !!!"
    exit 1
fi

# Check API server endpoint
if echo "${STATUS_OUTPUT}" | grep -q "API Server.*https://${HOST_IP}:6443"; then
    echo "✓ API server endpoint is correct"
else
    echo "!!! FAIL: API server endpoint mismatch !!!"
    exit 1
fi

# Check node name
if echo "${STATUS_OUTPUT}" | grep -q "Node Name.*keelnode-test"; then
    echo "✓ Node name is correct"
else
    echo "!!! FAIL: Node name mismatch !!!"
    exit 1
fi

# Debug: Check for CSRs from the node
echo ""
echo "Checking for Certificate Signing Requests..."
kubectl get csr -o wide || true
CSR_COUNT=$(kubectl get csr 2>/dev/null | grep -c keelnode || echo "0")
echo "CSRs found: ${CSR_COUNT}"

# Debug: Check if kubelet is actually running in QEMU
echo ""
echo "Checking QEMU logs for kubelet activity..."
if grep -q "Starting kubelet" "${LOG_FILE}" 2>/dev/null; then
    echo "✓ Kubelet startup detected in logs"
    # Show last few kubelet-related log lines
    echo "Recent kubelet logs:"
    grep "kubelet" "${LOG_FILE}" 2>/dev/null | tail -10 || true
else
    echo "⚠ No kubelet startup messages found in logs"
fi

# Debug: Verify bootstrap config was applied
echo ""
echo "Verifying bootstrap configuration in guest..."
VERIFY_OUTPUT=$("${OSCTL}" --endpoint "${ENDPOINT}" bootstrap-status 2>&1)
if echo "${VERIFY_OUTPUT}" | grep -q "ca.crt"; then
    echo "✓ CA certificate path detected in config"
else
    echo "⚠ CA certificate may not be configured"
fi

# Wait for node to appear in cluster
echo ""
echo "Checking if node appears in cluster (waiting up to 90s)..."
NODE_TIMEOUT=90
START_TIME=$(date +%s)
NODE_NAME="keelnode-test"
NODE_JOINED=0

while true; do
    CURRENT_TIME=$(date +%s)
    ELAPSED=$((CURRENT_TIME - START_TIME))
    
    if [ $ELAPSED -gt $NODE_TIMEOUT ]; then
        echo "✗ FAIL: Node did not appear in cluster after ${NODE_TIMEOUT}s"
        echo "  Expected: Node '${NODE_NAME}' should appear in 'kubectl get nodes'"
        echo "  Actual: Node not found"
        echo ""
        echo "Debug information:"
        echo "  CSRs found: ${CSR_COUNT}"
        kubectl get csr -o wide 2>/dev/null || echo "  No CSRs"
        echo ""
        echo "This indicates kubelet failed to join the cluster."
        cleanup
        exit 1
    fi
    
    if kubectl get nodes "${NODE_NAME}" &>/dev/null; then
        NODE_JOINED=1
        NODE_STATUS=$(kubectl get node "${NODE_NAME}" -o jsonpath='{.status.conditions[?(@.type=="Ready")].status}')
        echo "✓ Node joined cluster with status: ${NODE_STATUS}"
        kubectl get nodes "${NODE_NAME}"
        break
    fi
    
    sleep 3
done

# Wait for node to become Ready
echo ""
echo "Waiting for node to become Ready (CNI up)..."
READY_TIMEOUT=600
START_TIME=$(date +%s)
NODE_READY_FLAG=0

while true; do
    CURRENT_TIME=$(date +%s)
    ELAPSED=$((CURRENT_TIME - START_TIME))
    
    if [ $ELAPSED -gt $READY_TIMEOUT ]; then
        echo "✗ FAIL: Node is not Ready after ${READY_TIMEOUT}s"
        echo "  This likely means CNI failed to start, possibly due to container runtime errors."
        echo "  Node status:"
        kubectl get nodes "${NODE_NAME}" -o wide
        echo "  Node partial description:"
        kubectl describe node "${NODE_NAME}" | grep -A 10 "Conditions"
        echo ""
        echo "  === OCI/Runtime errors from QEMU log ==="
        grep -i "OCI runtime\|pivot_root\|runc create failed\|RunPodSandbox.*failed\|StartContainer.*failed" "${LOG_FILE}" 2>/dev/null | tail -20 || echo "  (no OCI errors found in log)"
        echo "  === End OCI errors ==="
        echo ""
        echo "  === Persistent storage status ==="
        grep -i "persistent storage\|mount.*data\|mkfs\|Bind-mounted" "${LOG_FILE}" 2>/dev/null | tail -10 || echo "  (no storage messages found)"
        echo "  === End storage status ==="
        echo ""
        echo "  === Image import status ==="
        grep -i "Scanning\|import\|pre-loaded\|images dir\|data.images\|share.*keel\|Successfully" "${LOG_FILE}" 2>/dev/null | tail -20 || echo "  (no import messages found)"
        echo "  === End image import status ==="
        echo ""
        echo "  === Pod status ==="
        kubectl get pods -A -o wide 2>/dev/null || echo "  (could not get pod status)"
        echo "  === End pod status ==="
        echo ""
        echo "  === Pod events ==="
        kubectl get events -n kube-system --sort-by=.lastTimestamp 2>/dev/null | tail -20 || echo "  (could not get events)"
        echo "  === End pod events ==="
        echo ""
        echo "  === Kindnet pod logs (all pods) ==="
        for pod in $(kubectl get pods -n kube-system -o name 2>/dev/null | grep kindnet); do
            podname=$(echo "$pod" | sed 's|pod/||')
            node=$(kubectl get "$pod" -n kube-system -o jsonpath='{.spec.nodeName}' 2>/dev/null || echo "unknown")
            phase=$(kubectl get "$pod" -n kube-system -o jsonpath='{.status.phase}' 2>/dev/null || echo "unknown")
            echo "  --- ${podname} (node=${node}, phase=${phase}) ---"
            echo "  Describe:"
            kubectl describe pod -n kube-system "$podname" 2>/dev/null | grep -A 5 "State:\|Last State:\|Reason:\|Exit Code:\|Message:" | head -20
            echo "  Current logs:"
            kubectl logs -n kube-system "$podname" 2>/dev/null | tail -20 || echo "  (no current logs)"
            echo "  Previous logs:"
            kubectl logs -n kube-system "$podname" --previous 2>/dev/null | tail -20 || echo "  (no previous logs)"
            echo ""
        done
        echo "  === End kindnet logs ==="
        cleanup
        exit 1
    fi
    
    NODE_READY_STATUS=$(kubectl get node "${NODE_NAME}" -o jsonpath='{.status.conditions[?(@.type=="Ready")].status}')
    if [ "${NODE_READY_STATUS}" = "True" ]; then
        echo "✓ Node is Ready!"
        NODE_READY_FLAG=1
        break
    fi
    
    echo "  Waiting for Ready status (Current: ${NODE_READY_STATUS}, elapsed: ${ELAPSED}s)..."
    sleep 5
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
if [ $NODE_JOINED -eq 1 ]; then
    echo "✓ Node joined cluster successfully"
fi
if [ $NODE_READY_FLAG -eq 1 ]; then
    echo "✓ Node became Ready (CNI active)"
fi
echo "========================================="
echo ">>> PASS: Bootstrap test completed successfully!"
echo "========================================="
