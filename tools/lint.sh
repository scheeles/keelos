#!/bin/bash
set -e

# This script runs cargo clippy inside a Docker container
# It uses the project's builder image to avoid installing dependencies on every run
# It also uses persistent volumes for cargo cache and target directory to speed up builds

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
IMAGE_NAME="keelos-builder"

# Check if builder image exists
if ! docker image inspect "${IMAGE_NAME}" >/dev/null 2>&1; then
    echo ">>> Builder image '${IMAGE_NAME}' not found. Building it first..."
    "${PROJECT_ROOT}/tools/builder/build.sh"
    # Note: build.sh drops us into a shell, exit it if that happens or modify build.sh?
    # Actually build.sh drops into a shell at the end. We should just build the image manually here if needed or ask user to run build.sh first?
    # Let's just build it here without entering the shell.
    
    # Simple build command matching build.sh logic
    HOST_ARCH=$(uname -m)
    K8S_VERSION=${K8S_VERSION:-v1.29.0}
    if [[ "${HOST_ARCH}" == "arm64" ]] || [[ "${HOST_ARCH}" == "aarch64" ]]; then
        docker build --platform linux/arm64 --build-arg K8S_VERSION="${K8S_VERSION}" -t "${IMAGE_NAME}" "${PROJECT_ROOT}/tools/builder"
    else
        docker build --build-arg K8S_VERSION="${K8S_VERSION}" -t "${IMAGE_NAME}" "${PROJECT_ROOT}/tools/builder"
    fi
fi

echo ">>> Running cargo clippy in Docker (${IMAGE_NAME})..."

# Use the same volumes as build.sh for consistency and caching
docker run --rm \
    -v "${PROJECT_ROOT}:/keelos" \
    -v "keelos-cargo-cache:/root/.cargo/registry" \
    -v "keelos-target-cache:/keelos/target" \
    -w /keelos \
    "${IMAGE_NAME}" \
    /bin/bash -c "
    cargo clippy --workspace -- -D warnings
"
echo ">>> Running cargo fmt in Docker (${IMAGE_NAME})..."
docker run --rm \
    -v "${PROJECT_ROOT}:/keelos" \
    -v "keelos-cargo-cache:/root/.cargo/registry" \
    -v "keelos-target-cache:/keelos/target" \
    -w /keelos \
    "${IMAGE_NAME}" \
    /bin/bash -c "
    cargo fmt --all -- --check
"