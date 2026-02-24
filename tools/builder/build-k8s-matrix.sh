#!/bin/bash
set -e

# Build KeelOS images for all supported Kubernetes versions.
#
# This script iterates over the supported K8s version matrix and invokes the
# variant build (or the base build) for each version, producing one set of
# artifacts per Kubernetes release.
#
# Usage:
#   ./tools/builder/build-k8s-matrix.sh [variant] [version]
#
# Arguments:
#   variant  - Image variant: cloud, bare-metal, edge, dev (default: cloud)
#   version  - KeelOS version string (default: dev)
#
# Environment variables:
#   K8S_VERSIONS       - Space-separated list of K8s versions to build
#                        (default: "v1.30.0 v1.31.0 v1.32.0")
#   SKIP_KERNEL        - Set to 1 to skip kernel build (reuse existing)
#   SKIP_CARGO         - Set to 1 to skip cargo build (reuse existing)
#
# Examples:
#   # Build cloud variant for all supported K8s versions
#   ./tools/builder/build-k8s-matrix.sh cloud v1.0.0
#
#   # Build only specific versions
#   K8S_VERSIONS="v1.31.0 v1.32.0" ./tools/builder/build-k8s-matrix.sh edge v1.0.0

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

# KeelOS supported Kubernetes versions (N, N-1, N-2)
K8S_VERSIONS="${K8S_VERSIONS:-v1.30.0 v1.31.0 v1.32.0}"

VARIANT="${1:-cloud}"
VERSION="${2:-dev}"

echo "============================================================"
echo " KeelOS Multi-Kubernetes Version Builder"
echo "============================================================"
echo " Variant:    ${VARIANT}"
echo " Version:    ${VERSION}"
echo " K8s Matrix: ${K8S_VERSIONS}"
echo "============================================================"
echo ""

FAILED=""

for K8S_VER in ${K8S_VERSIONS}; do
    echo ">>> Building for Kubernetes ${K8S_VER}..."
    echo "------------------------------------------------------------"

    if K8S_VERSION="${K8S_VER}" \
        "${PROJECT_ROOT}/tools/builder/build-variant.sh" "${VARIANT}" "${VERSION}"; then
        echo ">>> ✓ Kubernetes ${K8S_VER} build succeeded"
    else
        echo ">>> ✗ Kubernetes ${K8S_VER} build FAILED"
        FAILED="${FAILED} ${K8S_VER}"
    fi
    echo ""
done

echo "============================================================"
echo " Build Matrix Summary"
echo "============================================================"
for K8S_VER in ${K8S_VERSIONS}; do
    if echo "${FAILED}" | grep -qw "${K8S_VER}"; then
        echo "  ${K8S_VER}: FAILED"
    else
        echo "  ${K8S_VER}: OK"
    fi
done

if [ -n "${FAILED}" ]; then
    echo ""
    echo "ERROR: Builds failed for:${FAILED}"
    exit 1
fi

echo ""
echo "All Kubernetes version builds succeeded."
