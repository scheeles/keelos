#!/bin/zsh
set -e

# Project root is 2 levels up from this script (tools/builder)
# zsh/bash compatible way to find directory of this script
SCRIPT_DIR=${0:a:h}
PROJECT_ROOT="${SCRIPT_DIR}/../.."
IMAGE_NAME="maticos-builder"

# Detect host architecture
HOST_ARCH=$(uname -m)
echo "=== Detected host architecture: ${HOST_ARCH} ==="

K8S_VERSION=${K8S_VERSION:-v1.29.0}
echo "=== Building Builder Image (K8S_VERSION=${K8S_VERSION}) ==="

# Build with platform detection
# Always target x86_64 for MaticOS, but build container for host platform
if [[ "${HOST_ARCH}" == "arm64" ]] || [[ "${HOST_ARCH}" == "aarch64" ]]; then
    echo "=== Building on ARM64 (Apple Silicon) ==="
    echo "Note: Container runs on ARM, but will cross-compile to x86_64"
    docker build \
        --platform linux/arm64 \
        --build-arg K8S_VERSION="${K8S_VERSION}" \
        -t "${IMAGE_NAME}" \
        "${PROJECT_ROOT}/tools/builder"
else
    echo "=== Building on x86_64 ==="
    docker build \
        --build-arg K8S_VERSION="${K8S_VERSION}" \
        -t "${IMAGE_NAME}" \
        "${PROJECT_ROOT}/tools/builder"
fi

echo "=== Entering Build Environment ==="
# We mount the project root into /maticos
# We map the cargo cache to speed up builds
docker run --rm -it \
    -v "${PROJECT_ROOT}:/maticos" \
    -v "maticos-cargo-cache:/root/.cargo/registry" \
    -v "maticos-target-cache:/maticos/target" \
    --privileged \
    "${IMAGE_NAME}" \
    /bin/bash
