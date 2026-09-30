# Hockey Stats

A Mac, Windows and Linux app that turns InStat's post-game PDFs (the Match report and the Players' report) into
line, defence-pair and player analysis: shot maps, puck battles by area, practice focus areas,
rankings, and the statistics behind them. Everything runs locally; reports never leave the
computer.

**Download:** https://willmccallion.github.io/instat-analysis/

## How it works

Double-clicking the app starts a small server on `127.0.0.1` and opens the browser. Dropped
PDFs are parsed in Rust (word positions from `pdf-extract`), reconciled across both reports,
and stored in `~/Library/Application Support/HockeyStats` (Mac),
`%LOCALAPPDATA%\HockeyStats` (Windows) or `~/.local/share/hockey-stats` (Linux). The UI in `web/` is plain
JavaScript and SVG, embedded in the binary.

## Building

Everything needed is in the Nix dev shell, including cross-compilation to macOS and Windows:

```sh
nix develop
cargo test
scripts/bundle-mac.sh       # dist/Hockey-Stats-mac.dmg and .zip (universal, ad-hoc signed)
scripts/bundle-windows.sh   # dist/Hockey-Stats-windows.exe (x86-64, unsigned)
scripts/bundle-linux.sh     # dist/Hockey-Stats-linux (static x86-64)
```

Linux users install with `docs/install.sh`, served from the download page.

`cargo run -- --report game_match.pdf game_players.pdf --team SSAC -o report.html` builds a
standalone report without the app.

## Releasing

With everything committed, pass the next version (one step up: patch, minor or major) and the notes:

```sh
scripts/release.sh 1.2.1 "What changed"
```

This bumps `Cargo.toml` and `Cargo.lock`, commits "Release 1.2.1", builds all three apps, signs the Mac update zip, the Windows `.exe` and the Linux program with the release key
(minisign; kept outside the repo) and publishes a GitHub release. Installed apps offer the update on their next launch and refuse any download
whose signature doesn't match the key in `src/update.rs`.

## Test data

The end-to-end tests run against real InStat reports, which name players, so they are kept out
of the repository (`tests/fixtures/` and `tests/game1.rs` are ignored).
