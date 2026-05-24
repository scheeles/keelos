#!/bin/bash
set -e

# Project root is 2 levels up from this script (tools/builder)
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="${SCRIPT_DIR}/../.."
IMAGE_NAME="keelos-builder"

# Parse arguments
NO_INTERACTIVE=false
for arg in "$@"; do
    case "$arg" in
        --no-interactive)
            NO_INTERACTIVE=true
            ;;
    esac
done

# Detect host architecture
HOST_ARCH=$(uname -m)
echo "=== Detected host architecture: ${HOST_ARCH} ==="

K8S_VERSION=${K8S_VERSION:-v1.31.0}
echo "=== Building Builder Image (K8S_VERSION=${K8S_VERSION}) ==="

# Build with platform detection
# Always target x86_64 for KeelOS, but build container for host platform
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

# If --no-interactive is passed or stdin is not a TTY, just build and exit
if [ "${NO_INTERACTIVE}" = true ] || [ ! -t 0 ]; then
    echo "=== Builder image built successfully ==="
    exit 0
fi

echo "=== Entering Build Environment ==="
# We mount the project root into /keelos
# We map the cargo cache to speed up builds
docker run --rm -it \
    -v "${PROJECT_ROOT}:/keelos" \
    -v "keelos-cargo-cache:/root/.cargo/registry" \
    -v "keelos-target-cache:/keelos/target" \
    --privileged \
    "${IMAGE_NAME}" \
    /bin/bash
