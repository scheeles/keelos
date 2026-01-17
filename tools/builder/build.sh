#!/bin/bash
set -e

# Project root is 2 levels up from this script (tools/builder)
PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
IMAGE_NAME="maticos-builder"

echo "=== Building Builder Image ==="
docker build -t "${IMAGE_NAME}" "${PROJECT_ROOT}/tools/builder"

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
