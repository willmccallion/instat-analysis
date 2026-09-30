#!/usr/bin/env bash
# Publishes the version in Cargo.toml as a GitHub release: builds the Mac, Windows and Linux apps,
# signs the files in-app updates download, tags, and uploads. Run inside `nix develop` with
# `gh` logged in.
#   scripts/release.sh "What changed, in a sentence or two for the coaches"
set -euo pipefail

cd "$(dirname "$0")/.."
notes="${1:?usage: scripts/release.sh \"what changed\"}"
key="${HOCKEY_RELEASE_KEY:-$HOME/.config/hockey-stats-release/minisign.key}"
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
tag="v$version"
zip=dist/Hockey-Stats-mac.zip
exe=dist/Hockey-Stats-windows.exe
linux=dist/Hockey-Stats-linux

if [ -n "$(git status --porcelain)" ]; then
  echo "commit or stash your changes first" >&2
  exit 1
fi
if git rev-parse -q --verify "refs/tags/$tag" >/dev/null; then
  echo "$tag already exists; bump the version in Cargo.toml" >&2
  exit 1
fi

public_key=$(sed -n 's/^const PUBLIC_KEY: &str = "\(.*\)";/\1/p' src/update.rs)
if ! grep -qx "public_key=$public_key" docs/install.sh; then
  echo "docs/install.sh checks downloads against a different key than src/update.rs" >&2
  exit 1
fi

./scripts/bundle-mac.sh
./scripts/bundle-windows.sh
./scripts/bundle-linux.sh

for file in "$zip" "$exe" "$linux"; do
  # Must match update::trusted_comment, which the app checks before installing.
  minisign -S -s "$key" -m "$file" -x "$file.minisig" -t "hockey-stats $version $(basename "$file")"
  minisign -V -q -P "$public_key" -m "$file" -x "$file.minisig"
done

git tag -a "$tag" -m "Hockey Stats $version"
git push origin HEAD "$tag"
gh release create "$tag" dist/Hockey-Stats-mac.dmg "$zip" "$zip.minisig" "$exe" "$exe.minisig" "$linux" "$linux.minisig" \
  --title "Hockey Stats $version" --notes "$notes"
echo "Released $tag"
