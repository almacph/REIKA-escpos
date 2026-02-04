#!/bin/bash
# Build script for Windows executable

set -e

# Source cargo environment
if [ -f "$HOME/.cargo/env" ]; then
    source "$HOME/.cargo/env"
fi

echo "Building REIKA Printer Service for Windows..."

# Ensure Windows target is installed
rustup target add x86_64-pc-windows-gnu 2>/dev/null || true

# Build release for Windows
cargo build --release --target x86_64-pc-windows-gnu

echo ""

# Create release zip with exe and watchdog
RELEASE_DIR="target/x86_64-pc-windows-gnu/release"
cp reika-watchdog.vbs "$RELEASE_DIR/"

cd "$RELEASE_DIR"
zip -j reika-escpos.zip reika-escpos.exe reika-watchdog.vbs
cd - > /dev/null

echo "Build complete!"
echo "Release package:"
ls -lh "$RELEASE_DIR/reika-escpos.zip"
