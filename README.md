# Hockey Stats

A Mac app that turns InStat's post-game PDFs (the Match report and the Players' report) into
line, defence-pair and player analysis: shot maps, puck battles by area, practice focus areas,
rankings, and the statistics behind them. Everything runs locally; reports never leave the
computer.

**Download:** https://willmccallion.github.io/instat-analysis/

## How it works

Double-clicking the app starts a small server on `127.0.0.1` and opens the browser. Dropped
PDFs are parsed in Rust (word positions from `pdf-extract`), reconciled across both reports,
and stored in `~/Library/Application Support/HockeyStats`. The UI in `web/` is plain
JavaScript and SVG, embedded in the binary.

## Building

Everything needed is in the Nix dev shell, including cross-compilation to macOS:

```sh
nix develop
cargo test
scripts/bundle-mac.sh   # dist/Hockey-Stats-mac.dmg and .zip (universal, ad-hoc signed)
```

`cargo run -- --report game_match.pdf game_players.pdf --team SSAC -o report.html` builds a
standalone report without the app.

## Releasing

Bump `version` in `Cargo.toml`, commit, then:

```sh
scripts/release.sh "What changed"
```

This signs the update zip with the release key (minisign; kept outside the repo) and publishes a
GitHub release. Installed apps offer the update on their next launch and refuse any download
whose signature doesn't match the key in `src/update.rs`.

## Test data

The end-to-end tests run against real InStat reports, which name players, so they are kept out
of the repository (`tests/fixtures/` and `tests/game1.rs` are ignored).
