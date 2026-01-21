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
echo "Build complete!"
echo "Executable: target/x86_64-pc-windows-gnu/release/reika-escpos.exe"
ls -lh target/x86_64-pc-windows-gnu/release/reika-escpos.exe
