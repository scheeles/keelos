#!/bin/bash
set -e

# Build osctl for multiple platforms using cross-rs
# Usage: ./tools/build-osctl.sh [target...]
# If no targets specified, builds all platforms

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUTPUT_DIR="${PROJECT_ROOT}/build/osctl"

# All supported targets
ALL_TARGETS=(
    "aarch64-apple-darwin"        # Darwin_arm64
    "x86_64-apple-darwin"         # Darwin_x86
    "aarch64-unknown-linux-musl"  # Linux_arm64
    "arm-unknown-linux-musleabihf"    # Linux_armv6
    "armv7-unknown-linux-musleabihf"  # Linux_armv7
    "x86_64-unknown-linux-musl"   # Linux_x86
    "x86_64-pc-windows-gnu"       # Windows_x86
)

# Map Rust targets to friendly names
get_friendly_name() {
    case "$1" in
        "aarch64-apple-darwin") echo "Darwin_arm64" ;;
        "x86_64-apple-darwin") echo "Darwin_x86" ;;
        "aarch64-unknown-linux-musl") echo "Linux_arm64" ;;
        "arm-unknown-linux-musleabihf") echo "Linux_armv6" ;;
        "armv7-unknown-linux-musleabihf") echo "Linux_armv7" ;;
        "x86_64-unknown-linux-musl") echo "Linux_x86" ;;
        "x86_64-pc-windows-gnu") echo "Windows_x86" ;;
        *) echo "$1" ;;
    esac
}

# Get binary extension
get_extension() {
    case "$1" in
        *windows*) echo ".exe" ;;
        *) echo "" ;;
    esac
}

# Check if target needs cross (non-native)
needs_cross() {
    local target="$1"
    local host_arch=$(uname -m)
    local host_os=$(uname -s)
    
    # macOS targets can be built natively on macOS
    if [[ "$host_os" == "Darwin" ]]; then
        case "$target" in
            *apple-darwin*) return 1 ;;  # Native build
        esac
    fi
    
    # Linux x86_64 can be built natively on Linux x86_64
    if [[ "$host_os" == "Linux" && "$host_arch" == "x86_64" ]]; then
        case "$target" in
            "x86_64-unknown-linux-musl") return 1 ;;  # Native build with musl
        esac
    fi
    
    return 0  # Needs cross
}

# Install cross if needed
ensure_cross() {
    if ! command -v cross &> /dev/null; then
        echo ">>> Installing cross-rs..."
        cargo install cross --git https://github.com/cross-rs/cross
    fi
}

# Build for a single target
build_target() {
    local target="$1"
    local friendly_name=$(get_friendly_name "$target")
    local ext=$(get_extension "$target")
    
    echo ">>> Building osctl for ${friendly_name} (${target})..."
    
    mkdir -p "${OUTPUT_DIR}/${friendly_name}"
    
    if needs_cross "$target"; then
        ensure_cross
        cross build --release --package osctl --target "$target"
    else
        # Add target if not installed
        rustup target add "$target" 2>/dev/null || true
        cargo build --release --package osctl --target "$target"
    fi
    
    # Copy binary to output directory
    local src="${PROJECT_ROOT}/target/${target}/release/osctl${ext}"
    local dst="${OUTPUT_DIR}/${friendly_name}/osctl${ext}"
    
    if [[ -f "$src" ]]; then
        cp "$src" "$dst"
        echo "    Created: ${dst}"
    else
        echo "    ERROR: Binary not found at ${src}"
        return 1
    fi
}

# Main
cd "$PROJECT_ROOT"
mkdir -p "$OUTPUT_DIR"

# Determine targets to build
if [[ $# -gt 0 ]]; then
    TARGETS=("$@")
else
    TARGETS=("${ALL_TARGETS[@]}")
fi

echo ">>> Building osctl for ${#TARGETS[@]} target(s)..."
echo ""

for target in "${TARGETS[@]}"; do
    build_target "$target"
    echo ""
done

echo ">>> Build complete! Binaries are in: ${OUTPUT_DIR}"
ls -la "${OUTPUT_DIR}"
