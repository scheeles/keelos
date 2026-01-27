#!/bin/bash
# Generate a binary delta file between two SquashFS images
#
# Usage: generate-delta.sh OLD_IMAGE NEW_IMAGE OUTPUT_DELTA
#
# This script uses bsdiff to create a binary patch file that can
# transform OLD_IMAGE into NEW_IMAGE. The delta file is typically
# much smaller than the full NEW_IMAGE, saving bandwidth during updates.

set -e

if [ "$#" -ne 3 ]; then
    echo "Usage: $0 OLD_IMAGE NEW_IMAGE OUTPUT_DELTA"
    echo ""
    echo "Example:"
    echo "  $0 os-v1.0.squashfs os-v1.1.squashfs update-v1.0-to-v1.1.delta"
    exit 1
fi

OLD_IMAGE="$1"
NEW_IMAGE="$2"
OUTPUT_DELTA="$3"

# Validate inputs
if [ ! -f "$OLD_IMAGE" ]; then
    echo "Error: Old image not found: $OLD_IMAGE"
    exit 1
fi

if [ ! -f "$NEW_IMAGE" ]; then
    echo "Error: New image not found: $NEW_IMAGE"
    exit 1
fi

echo ">>> Generating delta update file..."
echo "Old image: $OLD_IMAGE ($(stat -f%z "$OLD_IMAGE" 2>/dev/null || stat -c%s "$OLD_IMAGE" 2>/dev/null || echo "?") bytes)"
echo "New image: $NEW_IMAGE ($(stat -f%z "$NEW_IMAGE" 2>/dev/null || stat -c%s "$NEW_IMAGE" 2>/dev/null || echo "?") bytes)"

# Use Docker container to generate delta (ensures bsdiff is available)
PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

docker run --rm \
    -v "$PROJECT_ROOT:/keelos" \
    -v "$(dirname "$(realpath "$OLD_IMAGE")"):/input_old" \
    -v "$(dirname "$(realpath "$NEW_IMAGE")"):/input_new" \
    -v "$(dirname "$(realpath "$OUTPUT_DELTA")"):/output" \
    -w /keelos \
    keelos-builder \
    /bin/bash -c "
        set -e
        echo '>>> Building delta generation tool...'
        cargo build --release --package keel-agent
        
        echo '>>> Generating delta with bsdiff...'
        # Use a simple Rust script to call bsdiff library
        cat > /tmp/gen_delta.rs << 'EOF'
use std::fs;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 4 {
        eprintln!(\"Usage: {} OLD NEW DELTA\", args[0]);
        std::process::exit(1);
    }
    
    let old = fs::read(&args[1])?;
    let new = fs::read(&args[2])?;
    
    println!(\"Generating delta...\");
    let delta = bsdiff::diff(&old, &new)?;
    
    fs::write(&args[3], delta)?;
    println!(\"Delta written to: {}\", args[3]);
    
    Ok(())
}
EOF
        
        cat > /tmp/Cargo.toml << 'EOF'
[package]
name = \"gen_delta\"
version = \"0.1.0\"
edition = \"2021\"

[dependencies]
bsdiff = \"1.0\"
EOF
        
        cd /tmp
        cargo build --release
        
        /tmp/target/release/gen_delta \
            \"/input_old/$(basename "$OLD_IMAGE")\" \
            \"/input_new/$(basename "$NEW_IMAGE")\" \
            \"/output/$(basename "$OUTPUT_DELTA")\"
    "

# Calculate statistics
OLD_SIZE=$(stat -f%z "$OLD_IMAGE" 2>/dev/null || stat -c%s "$OLD_IMAGE" 2>/dev/null)
NEW_SIZE=$(stat -f%z "$NEW_IMAGE" 2>/dev/null || stat -c%s "$NEW_IMAGE" 2>/dev/null)
DELTA_SIZE=$(stat -f%z "$OUTPUT_DELTA" 2>/dev/null || stat -c%s "$OUTPUT_DELTA" 2>/dev/null)

echo ""
echo ">>> Delta generation complete!"
echo "Delta file: $OUTPUT_DELTA ($DELTA_SIZE bytes)"

if [ -n "$NEW_SIZE" ] && [ "$NEW_SIZE" -gt 0 ]; then
    SAVINGS_PERCENT=$(echo "scale=2; (1 - $DELTA_SIZE / $NEW_SIZE) * 100" | bc)
    SAVINGS_MB=$(echo "scale=2; ($NEW_SIZE - $DELTA_SIZE) / 1024 / 1024" | bc)
    echo "Bandwidth savings: ${SAVINGS_MB} MB (${SAVINGS_PERCENT}% reduction vs full download)"
fi

echo ""
echo "To apply this delta update, use:"
echo "  osctl update --source http://server/$OUTPUT_DELTA --delta --fallback --full-image-url http://server/$NEW_IMAGE"
