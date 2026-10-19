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
cargo build --locked --release --target x86_64-pc-windows-gnu

echo ""

# Create release zip with exe and watchdog
RELEASE_DIR="target/x86_64-pc-windows-gnu/release"
cp reika-watchdog.vbs "$RELEASE_DIR/"
cp installer/reika-user-launcher.cmd "$RELEASE_DIR/"

cd "$RELEASE_DIR"
if command -v zip >/dev/null 2>&1; then
  zip -j reika-escpos.zip reika-escpos.exe reika-driver-setup.exe reika-watchdog.vbs reika-user-launcher.cmd
else
  python3 - reika-escpos.zip reika-escpos.exe reika-driver-setup.exe reika-watchdog.vbs reika-user-launcher.cmd <<'PY'
import sys
from pathlib import Path
from zipfile import ZIP_DEFLATED, ZipFile

archive, *members = sys.argv[1:]
with ZipFile(archive, "w", compression=ZIP_DEFLATED) as output:
    for member in members:
        path = Path(member)
        output.write(path, arcname=path.name)
PY
fi
cd - > /dev/null

# Build the elevated Inno Setup installer only when the reviewed signed driver
# package is present. Portable ZIP output remains available for development.
if command -v ISCC.exe >/dev/null 2>&1 && test -f installer/driver/standard.inf \
  && test -f installer/driver/standard.cat \
  && test -f installer/driver/ics-advent.inf \
  && test -f installer/driver/ics-advent.cat \
  && test -f installer/driver/signing-provenance.json; then
  ISCC.exe installer/reika-setup.iss
else
  if [ "${REIKA_REQUIRE_INSTALLER:-0}" = "1" ]; then
    echo "Production installer requested but Inno Setup or signed INF/CAT assets are unavailable." >&2
    exit 1
  fi
  echo "Signed INF/CAT assets or Inno Setup compiler unavailable; skipping production installer (portable ZIP retained)."
fi

bash ./scripts/release-manifest.sh "$RELEASE_DIR/reika-escpos.zip" "$RELEASE_DIR/reika-escpos.provenance.json"

echo "Build complete!"
echo "Release package:"
ls -lh "$RELEASE_DIR/reika-escpos.zip"
