#!/bin/sh
# Installs Hockey Stats for the current user: the app in ~/.local/bin and an entry in the
# applications menu. Run it again to update by hand; the app also updates itself.
#   curl -fsSL https://willmccallion.github.io/instat-analysis/install.sh | sh
#   curl -fsSL https://willmccallion.github.io/instat-analysis/install.sh | sh -s -- --uninstall
set -eu

release=https://github.com/willmccallion/instat-analysis/releases/latest/download
site=https://willmccallion.github.io/instat-analysis
# Must match PUBLIC_KEY in src/update.rs (scripts/release.sh checks this).
public_key=RWTxOXCn0SiJjkAsQAVAkTqw1AdcEbblc4yELiY1bBQhH935ZXLevaLB

bin_dir=${XDG_BIN_HOME:-$HOME/.local/bin}
data_home=${XDG_DATA_HOME:-$HOME/.local/share}
app=$bin_dir/hockey-stats
menu_entry=$data_home/applications/hockey-stats.desktop
icon=$data_home/icons/hicolor/scalable/apps/hockey-stats.svg

fail() {
  echo "Hockey Stats: $1" >&2
  exit 1
}

uninstall() {
  rm -f "$app" "$menu_entry" "$icon"
  echo "Hockey Stats is uninstalled."
  echo "Your games are still in $data_home/hockey-stats; delete that folder to remove them too."
}

case "${1:-}" in
  --uninstall) uninstall; exit 0 ;;
  "") ;;
  *) fail "unknown option $1 (use --uninstall to remove the app)" ;;
esac

[ "$(uname -s)" = Linux ] || fail "this installer is for Linux; get the Mac or Windows app from $site"
case "$(uname -m)" in
  x86_64 | amd64) ;;
  *) fail "the Linux app needs a 64-bit Intel or AMD processor (this computer has $(uname -m))" ;;
esac
command -v curl >/dev/null 2>&1 || fail "please install curl first (for example: sudo apt install curl)"

mkdir -p "$bin_dir" "$(dirname "$menu_entry")" "$(dirname "$icon")"
download=$(mktemp "$bin_dir/.hockey-stats.XXXXXX")
trap 'rm -f "$download" "$download.minisig"' EXIT
echo "Downloading Hockey Stats..."
curl -fsSL "$release/Hockey-Stats-linux" -o "$download"
if command -v minisign >/dev/null 2>&1; then
  curl -fsSL "$release/Hockey-Stats-linux.minisig" -o "$download.minisig"
  minisign -V -q -P "$public_key" -m "$download" -x "$download.minisig" || fail "the download failed its signature check"
fi
chmod 755 "$download"
mv "$download" "$app"

curl -fsSL "$site/icon.svg" -o "$icon"
cat > "$menu_entry" <<ENTRY
[Desktop Entry]
Type=Application
Name=Hockey Stats
Comment=Turn InStat game reports into line, pairing and player analysis
Exec="$app" --background
Icon=$icon
Terminal=false
Categories=Education;Sports;
ENTRY
if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database -q "$(dirname "$menu_entry")" || true
fi

echo "Hockey Stats is installed. Open it from your applications menu, or run: $app"
case ":$PATH:" in
  *":$bin_dir:"*) ;;
  *) echo "($bin_dir isn't on your PATH, so use the full path above or the menu.)" ;;
esac
