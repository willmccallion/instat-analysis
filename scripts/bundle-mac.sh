#!/usr/bin/env bash
# Builds "Hockey Stats.app" (universal arm64 + x86_64), ad-hoc signs it and zips it with
# the coach README. Run inside `nix develop`.
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
(cd "$dist" && zip -qry "Hockey-Stats-$version-mac.zip" "Hockey Stats.app" README-coach.txt)
echo "Built $dist/Hockey-Stats-$version-mac.zip"
