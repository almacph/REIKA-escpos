#!/usr/bin/env bash
set -euo pipefail

artifact="${1:?artifact path required}"
output="${2:?output path required}"
version="$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -1)"
commit="$(git rev-parse HEAD)"
toolchain="$(rustc --version)"
hash="$(sha256sum "$artifact" | awk '{print $1}')"

cat > "$output" <<EOF
{
  "package": "reika-escpos",
  "version": "$version",
  "commit": "$commit",
  "toolchain": "$toolchain",
  "artifact": "$(basename "$artifact")",
  "sha256": "$hash"
}
EOF
