#!/bin/bash
set -e

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
KIND_CONFIG="${PROJECT_ROOT}/build/kind-config.yaml"
CLUSTER_NAME="keel-test"

echo ">>> Setting up Kind cluster for KeelOS bootstrap testing..."

# Check if kind is installed
if ! command -v kind &> /dev/null; then
    echo "Error: kind is not installed. Please install it first:"
    echo "  https://kind.sigs.k8s.io/docs/user/quick-start/#installation"
    exit 1
fi

# Check if kubectl is installed
if ! command -v kubectl &> /dev/null; then
    echo "Error: kubectl is not installed."
    exit 1
fi

# Check if cluster already exists
if kind get clusters 2>/dev/null | grep -q "^${CLUSTER_NAME}$"; then
    echo "Cluster '${CLUSTER_NAME}' already exists. Skipping creation."
    exit 0
fi

# Create build directory if it doesn't exist
mkdir -p "${PROJECT_ROOT}/build"

# Generate kind configuration
echo "Generating kind configuration..."
cat > "${KIND_CONFIG}" <<EOF
kind: Cluster
apiVersion: kind.x-k8s.io/v1alpha4
networking:
  apiServerAddress: "127.0.0.1"
  apiServerPort: 6443
nodes:
- role: control-plane
  kubeadmConfigPatches:
  - |
    kind: ClusterConfiguration
    apiServer:
      certSANs:
      - "localhost"
      - "127.0.0.1"
      - "10.0.2.2"
EOF

echo "Creating kind cluster '${CLUSTER_NAME}'..."
kind create cluster --name "${CLUSTER_NAME}" --config "${KIND_CONFIG}"

echo "Waiting for cluster to be ready..."
kubectl wait --for=condition=Ready nodes --all --timeout=60s

# Create RBAC for bootstrap tokens (idempotent)
echo "Creating RBAC for bootstrap tokens..."

# Create ClusterRoleBinding for system:node-bootstrapper
kubectl create clusterrolebinding kubeadm:kubelet-bootstrap \
  --clusterrole=system:node-bootstrapper \
  --group=system:bootstrappers 2>/dev/null || echo "  ClusterRoleBinding already exists"

# Create RBAC for CSR auto-approval
kubectl create clusterrolebinding kubeadm:node-autoapprove-bootstrap \
  --clusterrole=system:certificates.k8s.io:certificatesigningrequests:nodeclient \
  --group=system:bootstrappers 2>/dev/null || echo "  CSR auto-approval already exists"

kubectl create clusterrolebinding kubeadm:node-autoapprove-certificate-rotation \
  --clusterrole=system:certificates.k8s.io:certificatesigningrequests:selfnodeclient \
  --group=system:nodes 2>/dev/null || echo "  Certificate rotation auto-approval already exists"

echo "✓ Kind cluster '${CLUSTER_NAME}' is ready!"
echo "  API Server: https://10.0.2.2:6443 (from QEMU guest)"
echo "  API Server: https://localhost:6443 (from host)"
