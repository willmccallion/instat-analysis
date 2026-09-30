#!/usr/bin/env bash
# Publishes a new version as a GitHub release: bumps Cargo.toml and Cargo.lock, commits
# "Release x.y.z", builds the Mac, Windows and Linux apps, signs the files in-app updates
# download, tags, and uploads. The version must be one step up from Cargo.toml's (patch,
# minor or major). Run inside `nix develop` with `gh` logged in.
#   scripts/release.sh 1.2.1 "What changed, in a sentence or two for the coaches"
set -euo pipefail

cd "$(dirname "$0")/.."
usage='usage: scripts/release.sh X.Y.Z "what changed"'
version="${1:?$usage}"
notes="${2:?$usage}"
key="${HOCKEY_RELEASE_KEY:-$HOME/.config/hockey-stats-release/minisign.key}"
current=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
tag="v$version"
zip=dist/Hockey-Stats-mac.zip
exe=dist/Hockey-Stats-windows.exe
linux=dist/Hockey-Stats-linux

if [[ ! "$version" =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]]; then
  echo "version must look like 1.2.3, not \"$version\"" >&2
  exit 1
fi
if [ -n "$(git status --porcelain)" ]; then
  echo "commit or stash your changes first" >&2
  exit 1
fi
if git rev-parse -q --verify "refs/tags/$tag" >/dev/null; then
  echo "$tag is already released" >&2
  exit 1
fi

IFS=. read -r major minor patch <<<"$current"
next_patch="$major.$minor.$((patch + 1))"
next_minor="$major.$((minor + 1)).0"
next_major="$((major + 1)).0.0"
resuming=false
if [ "$version" = "$current" ] && [ "$(git log -1 --format=%s)" = "Release $version" ]; then
  # An earlier run bumped and committed, then stopped before tagging.
  resuming=true
elif [ "$version" != "$next_patch" ] && [ "$version" != "$next_minor" ] && [ "$version" != "$next_major" ]; then
  echo "Cargo.toml is at $current; the next version is $next_patch, $next_minor or $next_major" >&2
  exit 1
fi

public_key=$(sed -n 's/^const PUBLIC_KEY: &str = "\(.*\)";/\1/p' src/update.rs)
if ! grep -qx "public_key=$public_key" docs/install.sh; then
  echo "docs/install.sh checks downloads against a different key than src/update.rs" >&2
  exit 1
fi

if [ "$resuming" = false ]; then
  sed -i "0,/^version = \"$current\"/s//version = \"$version\"/" Cargo.toml
  cargo update --workspace --offline --quiet
  git commit --quiet -m "Release $version" Cargo.toml Cargo.lock
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
