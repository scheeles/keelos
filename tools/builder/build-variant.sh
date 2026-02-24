#!/bin/bash
set -e

# Build a specific KeelOS image variant
#
# Usage: build-variant.sh <variant> [version]
#   variant: cloud, bare-metal, edge, dev (or path to custom .conf)
#   version: version string (default: dev)
#
# Environment variables:
#   SKIP_KERNEL  - Set to 1 to skip kernel build (reuse existing)
#   SKIP_CARGO   - Set to 1 to skip cargo build (reuse existing)
#   OUTPUT_DIR   - Override output directory (default: build/variants/<name>)
#
# Examples:
#   ./tools/builder/build-variant.sh cloud v1.0.0
#   ./tools/builder/build-variant.sh edge v1.0.0
#   SKIP_KERNEL=1 ./tools/builder/build-variant.sh dev

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
VARIANTS_DIR="${PROJECT_ROOT}/tools/builder/variants"

usage() {
    echo "Usage: $0 <variant> [version]"
    echo ""
    echo "Available variants:"
    for conf in "${VARIANTS_DIR}"/*.conf; do
        if [[ "$(basename "$conf")" == "base.conf" ]]; then
            continue
        fi
        # shellcheck disable=SC1090
        (source "$conf" && echo "  $(basename "$conf" .conf) - ${VARIANT_DESCRIPTION}")
    done
    echo ""
    echo "Options:"
    echo "  SKIP_KERNEL=1  Skip kernel build (reuse existing)"
    echo "  SKIP_CARGO=1   Skip cargo build (reuse existing)"
    exit 1
}

# =============================================================================
# Parse arguments
# =============================================================================

VARIANT="${1:-}"
VERSION="${2:-dev}"

if [[ -z "$VARIANT" ]]; then
    usage
fi

# Resolve variant config file
if [[ -f "$VARIANT" ]]; then
    VARIANT_CONF="$VARIANT"
elif [[ -f "${VARIANTS_DIR}/${VARIANT}.conf" ]]; then
    VARIANT_CONF="${VARIANTS_DIR}/${VARIANT}.conf"
else
    echo "ERROR: Unknown variant '${VARIANT}'"
    echo ""
    usage
fi

# =============================================================================
# Load configuration
# =============================================================================

# Load base defaults first
# shellcheck disable=SC1091
source "${VARIANTS_DIR}/base.conf"

# Override with variant-specific config
# shellcheck disable=SC1090
source "${VARIANT_CONF}"

echo "============================================================"
echo " KeelOS Image Variant Builder"
echo "============================================================"
echo " Variant:     ${VARIANT_NAME}"
echo " Description: ${VARIANT_DESCRIPTION}"
echo " Version:     ${VERSION}"
echo " Formats:     ${OUTPUT_FORMATS}"
echo "============================================================"
echo ""

# =============================================================================
# Setup directories
# =============================================================================

BUILD_DIR="${PROJECT_ROOT}/build"
VARIANT_BUILD_DIR="${OUTPUT_DIR:-${BUILD_DIR}/variants/${VARIANT_NAME}}"
VARIANT_KERNEL_DIR="${VARIANT_BUILD_DIR}/kernel"

mkdir -p "${VARIANT_BUILD_DIR}"
mkdir -p "${VARIANT_KERNEL_DIR}"

# =============================================================================
# Step 1: Build Rust binaries (shared across variants, unless profile differs)
# =============================================================================

if [[ "${SKIP_CARGO:-0}" != "1" ]]; then
    echo ">>> Building Rust components (profile: ${CARGO_PROFILE})..."

    CARGO_TARGET="x86_64-unknown-linux-musl"

    if [[ "${CARGO_PROFILE}" == "dev" ]]; then
        cargo build --target "${CARGO_TARGET}" --workspace
        TARGET_PROFILE_DIR="${PROJECT_ROOT}/target/${CARGO_TARGET}/debug"
    else
        cargo build --release --target "${CARGO_TARGET}" --workspace
        TARGET_PROFILE_DIR="${PROJECT_ROOT}/target/${CARGO_TARGET}/release"
    fi
else
    echo ">>> Skipping cargo build (SKIP_CARGO=1)"
    if [[ "${CARGO_PROFILE}" == "dev" ]]; then
        TARGET_PROFILE_DIR="${PROJECT_ROOT}/target/x86_64-unknown-linux-musl/debug"
    else
        TARGET_PROFILE_DIR="${PROJECT_ROOT}/target/x86_64-unknown-linux-musl/release"
    fi
fi

export VARIANT_TARGET_PROFILE_DIR="${TARGET_PROFILE_DIR}"

# =============================================================================
# Step 2: Build kernel with variant-specific config
# =============================================================================

if [[ "${SKIP_KERNEL:-0}" != "1" ]]; then
    echo ">>> Building kernel for variant '${VARIANT_NAME}'..."

    # Export variant kernel config for kernel-build.sh to pick up
    export VARIANT_KERNEL_EXTRA_ENABLE="${KERNEL_EXTRA_ENABLE}"
    export VARIANT_KERNEL_EXTRA_DISABLE="${KERNEL_EXTRA_DISABLE}"
    export VARIANT_KERNEL_DEBUG_INFO="${KERNEL_DEBUG_INFO}"
    export VARIANT_KERNEL_OUTPUT_DIR="${VARIANT_KERNEL_DIR}"

    "${PROJECT_ROOT}/tools/builder/kernel-build.sh"

    # Copy kernel to variant build dir if kernel-build.sh wrote to default location
    if [[ ! -f "${VARIANT_KERNEL_DIR}/bzImage" ]] && [[ -f "${BUILD_DIR}/kernel/bzImage" ]]; then
        cp "${BUILD_DIR}/kernel/bzImage" "${VARIANT_KERNEL_DIR}/bzImage"
    fi
else
    echo ">>> Skipping kernel build (SKIP_KERNEL=1)"
    # Use existing kernel
    if [[ ! -f "${VARIANT_KERNEL_DIR}/bzImage" ]]; then
        if [[ -f "${BUILD_DIR}/kernel/bzImage" ]]; then
            cp "${BUILD_DIR}/kernel/bzImage" "${VARIANT_KERNEL_DIR}/bzImage"
        else
            echo "ERROR: No kernel found. Run without SKIP_KERNEL or build kernel first."
            exit 1
        fi
    fi
fi

# =============================================================================
# Step 3: Build initramfs with variant-specific contents
# =============================================================================

echo ">>> Building initramfs for variant '${VARIANT_NAME}'..."

export VARIANT_NAME
export VARIANT_DEBUG_TOOLS="${INITRAMFS_DEBUG_TOOLS}"
export VARIANT_CLOUD_INIT="${INITRAMFS_CLOUD_INIT}"
export VARIANT_MINIMAL="${INITRAMFS_MINIMAL}"
export VARIANT_STRIP_BINARIES="${STRIP_BINARIES}"
export VARIANT_CARGO_PROFILE="${CARGO_PROFILE}"

"${PROJECT_ROOT}/tools/builder/initramfs-build.sh"

# =============================================================================
# Step 4: Build output images in requested formats
# =============================================================================

echo ">>> Building output images for variant '${VARIANT_NAME}'..."

export VARIANT_OUTPUT_FORMATS="${OUTPUT_FORMATS}"
export VARIANT_KERNEL_CMDLINE_EXTRA="${KERNEL_CMDLINE_EXTRA}"
export VARIANT_GRUB_TIMEOUT="${GRUB_TIMEOUT}"
export VARIANT_OUTPUT_DIR="${VARIANT_BUILD_DIR}"

"${PROJECT_ROOT}/tools/builder/build-variant-images.sh" "${VERSION}"

# =============================================================================
# Summary
# =============================================================================

echo ""
echo "============================================================"
echo " Build complete: ${VARIANT_NAME} variant (${VERSION})"
echo "============================================================"
echo ""
echo "Artifacts:"
ls -lh "${VARIANT_BUILD_DIR}"/keelos-* 2>/dev/null || echo "  (none)"
echo ""
echo "Location: ${VARIANT_BUILD_DIR}"
