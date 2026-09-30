#!/usr/bin/env bash
# Builds the Linux app: a static x86-64 program, dist/Hockey-Stats-linux, which is both the
# download (docs/install.sh fetches it) and what in-app updates fetch. Run inside `nix develop`.
set -euo pipefail

cd "$(dirname "$0")/.."
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)

cargo zigbuild --release --target x86_64-unknown-linux-musl
binary=target/x86_64-unknown-linux-musl/release/hockey-stats
if ldd "$binary" >/dev/null 2>&1; then
  echo "the Linux build is not static" >&2
  exit 1
fi

mkdir -p dist
cp "$binary" dist/Hockey-Stats-linux
echo "Built dist/Hockey-Stats-linux (version $version)"
