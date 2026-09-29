#!/usr/bin/env bash
# Builds "Hockey Stats.app" (universal arm64 + x86_64), ad-hoc signs it, and packages it as
# a drag-to-Applications disk image and a zip (the zip is what in-app updates download).
# Run inside `nix develop`.
set -euo pipefail

cd "$(dirname "$0")/.."
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
export MACOSX_DEPLOYMENT_TARGET=11.0
# Leave room in the Mach-O header for the code-signature load command rcodesign adds.
export CARGO_TARGET_AARCH64_APPLE_DARWIN_RUSTFLAGS="-C link-arg=-Wl,-headerpad_max_install_names"
export CARGO_TARGET_X86_64_APPLE_DARWIN_RUSTFLAGS="-C link-arg=-Wl,-headerpad_max_install_names"

# Cargo does not rebuild when the deployment target changes, so start clean.
rm -rf target/aarch64-apple-darwin target/x86_64-apple-darwin target/universal2-apple-darwin
cargo zigbuild --release --target universal2-apple-darwin

dist=dist
app="$dist/Hockey Stats.app"
rm -rf "$dist"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp target/universal2-apple-darwin/release/hockey-stats "$app/Contents/MacOS/hockey-stats"
sed "s/@VERSION@/$version/g" packaging/Info.plist > "$app/Contents/Info.plist"
printf 'APPL????' > "$app/Contents/PkgInfo"

renders="$dist/icon-renders"
mkdir -p "$renders"
for pixels in 16 32 64 128 256 512 1024; do
  rsvg-convert -w "$pixels" -h "$pixels" packaging/icon.svg -o "$renders/icon_$pixels.png"
done
python3 scripts/pack-icns.py "$renders" "$app/Contents/Resources/AppIcon.icns"
rm -rf "$renders"

# Apple Silicon refuses to run unsigned code; an ad-hoc signature is enough to launch
# (Gatekeeper still asks once, see README-coach.txt).
rcodesign sign "$app"
# rcodesign's `verify` misreports ad-hoc signatures (no CMS blob), so check the flags instead.
adhoc_slices=$(rcodesign print-signature-info "$app/Contents/MacOS/hockey-stats" | grep -c 'CodeSignatureFlags(ADHOC)')
if [ "$adhoc_slices" -ne 2 ]; then
  echo "expected an ad-hoc signature on both architectures, found $adhoc_slices" >&2
  exit 1
fi

cp packaging/README-coach.txt "$dist/README-coach.txt"
(cd "$dist" && zip -qry Hockey-Stats-mac.zip "Hockey Stats.app" README-coach.txt)

# The disk image shows the app next to an Applications shortcut to drag it onto. Rock Ridge
# (-r) keeps the shortcut and the executable bit; `dmg` turns the ISO into a compressed .dmg.
image="$dist/image"
mkdir -p "$image"
cp -R "$app" "$image/"
ln -s /Applications "$image/Applications"
xorrisofs -quiet -D -l -r -V "Hockey Stats" -no-pad -dir-mode 0755 -o "$dist/uncompressed.iso" "$image"
dmg "$dist/uncompressed.iso" "$dist/Hockey-Stats-mac.dmg" >/dev/null
rm -rf "$image" "$dist/uncompressed.iso"
echo "Built $dist/Hockey-Stats-mac.dmg and $dist/Hockey-Stats-mac.zip (version $version)"
