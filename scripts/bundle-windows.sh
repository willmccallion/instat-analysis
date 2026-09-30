#!/usr/bin/env bash
# Builds the Windows app: a single dist/Hockey-Stats-windows.exe with the app icon, which is
# both the download and what in-app updates fetch. Run inside `nix develop`.
set -euo pipefail

cd "$(dirname "$0")/.."
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)

resources=target/windows-resources
rm -rf "$resources"
mkdir -p "$resources"
for pixels in 16 32 64 128 256; do
  rsvg-convert -w "$pixels" -h "$pixels" packaging/icon.svg -o "$resources/icon_$pixels.png"
done
python3 scripts/pack-ico.py "$resources" "$resources/icon.ico"
printf '1 ICON "icon.ico"\n' > "$resources/icon.rc"
(cd "$resources" && zig rc /fo icon.res icon.rc)

export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUSTFLAGS="-C link-arg=$PWD/$resources/icon.res"
cargo zigbuild --release --target x86_64-pc-windows-gnu

exe=target/x86_64-pc-windows-gnu/release/hockey-stats.exe
if ! objdump -h "$exe" | grep -q '\.rsrc'; then
  echo "the Windows build has no icon resource" >&2
  exit 1
fi

mkdir -p dist
cp "$exe" dist/Hockey-Stats-windows.exe
echo "Built dist/Hockey-Stats-windows.exe (version $version)"
