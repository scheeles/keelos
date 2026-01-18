#!/bin/zsh
set -e

# Project root is 2 levels up from this script (tools/builder)
# zsh/bash compatible way to find directory of this script
SCRIPT_DIR=${0:a:h}
PROJECT_ROOT="${SCRIPT_DIR}/../.."
IMAGE_NAME="maticos-builder"

K8S_VERSION=${K8S_VERSION:-v1.29.0}
echo "=== Building Builder Image (K8S_VERSION=${K8S_VERSION}) ==="
docker build --context rancher-desktop --build-arg K8S_VERSION="${K8S_VERSION}" -t "${IMAGE_NAME}" "${PROJECT_ROOT}/tools/builder"

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
