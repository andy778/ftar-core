# ftar-core

[![CI](https://github.com/andy778/ftar-core/actions/workflows/ci.yml/badge.svg)](https://github.com/andy778/ftar-core/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/andy778/ftar-core?logo=github&cacheSeconds=300)](https://github.com/andy778/ftar-core/releases/latest)
[![Downloads](https://img.shields.io/github/downloads/andy778/ftar-core/total?logo=github&cacheSeconds=300)](https://github.com/andy778/ftar-core/releases)
[![License](https://img.shields.io/github/license/andy778/ftar-core?cacheSeconds=300)](LICENSE)
[![OpenSSF Scorecard](https://api.scorecard.dev/projects/github.com/andy778/ftar-core/badge)](https://scorecard.dev/viewer/?uri=github.com/andy778/ftar-core)

Rust port of the [OpenFTA](https://github.com/luyangshang/OpenFTA) fault tree analysis engine (unmaintained since 2013).
Java 21 baseline of the original: <https://github.com/andy778/OpenFTA/tree/dev>.

## Usage

```
cargo run --release -p ftar-cli -- prob crates/core/tests/data/theatre.fta
cargo run --release -p ftar-cli -- cutsets crates/core/tests/data/top.fta     # follows transfer-in trees
cargo run --release -p ftar-cli -- mc crates/core/tests/data/theatre.fta --trials 1000000
```

Commands: `info`, `cutsets`, `prob`, `mc`; options `--order --terms --time --trials --seed`.

## Status

- [x] `.fta` tree and `.ped` primary-event database parsers
- [x] Transfer-in / transfer-out trees (multi-file), cycle detection
- [x] Minimal cut sets with OpenFTA semantics (XOR analysed as OR, analysed/not-analysed conditions, order limit)
- [x] Exact top-event probability by inclusion-exclusion with per-term bounds, failure-rate (`L`) events via unit time, event importance
- [x] Monte Carlo simulation (reproducible per seed)
- [x] `ftar` CLI, JSON automation tools (`ftar tools|call|run`) and an MCP server (`ftar mcp --root DIR`, see docs/mcp.md)
- [x] Compare numbers with reports from the original engine: a native build of OpenFTA's C core (`tools/openfta-native/`) made the fixtures in `crates/core/tests/data/openfta/`; cut sets reports are identical, probability reports agree to float precision (docs/VERIFICATION.md)
- [x] Failure statistics report, text report files compatible with OpenFTA
- [x] Writing `.fta`/`.ped`, tree editing (GUI: add/delete/move/retype/rename, event database, OpenPED-style primary event editor (Edit > Primary events) that also lists the other trees in the folder using an event (an ftar addition; MCP/automation `event_usage`), undo/redo, save, unsaved-changes prompt)
- [x] GUI (egui): tree canvas, editing, File/Edit menus, analyses, report and Monte Carlo failure-mode export
- [x] Preferences (Edit > Preferences): pen, background, highlight and per-symbol fill colours plus monochrome mode, saved between sessions and used by the canvas, image export and print (headless: `ftar_render::Preferences`)
- [x] Image export (PNG/SVG) and printing: `ftar-render` draws the tree headlessly; GUI File > Export image / Print; `ftar draw`; MCP/automation `tree_export`
- [x] Fuzzing: cargo-fuzz targets for the `.fta`/`.ped` parsers, analysis and the MCP server, run on every PR by ClusterFuzzLite (fuzz/README.md)

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) (rules in [AGENTS.md](AGENTS.md)) and the [Code of Conduct](CODE_OF_CONDUCT.md).

## Releases

Pushing a tag `vX.Y.Z` (it must equal the `ftar-cli` version in `Cargo.toml`) runs `.github/workflows/release.yml`:
it builds `ftar` and `ftar-gui` with `cargo build --release --locked` for Linux (x86_64), Windows (x86_64) and macOS
(arm64), smoke-tests the CLI, packages each platform (binaries, LICENSE, NOTICE, README, docs), and publishes a GitHub release with
the archives, `SHA256SUMS.txt` and `ftar-vX.Y.Z.sigstore.json` (signed SLSA build provenance for every file, see below). The same workflow runs as a build-only dry run on manual dispatch and on pull requests that
touch `Cargo.toml`, `Cargo.lock` or the workflow.

### Verifying a download

Every release file has a build provenance attestation, signed with Sigstore by the release workflow. It proves the file was
built by `.github/workflows/release.yml` in this repository from the tagged commit. With the [GitHub CLI](https://cli.github.com):

```sh
gh attestation verify ftar-vX.Y.Z-x86_64-unknown-linux-gnu.tar.gz --repo andy778/ftar-core
# offline, with the bundle attached to the release:
gh attestation verify ftar-vX.Y.Z-x86_64-unknown-linux-gnu.tar.gz --repo andy778/ftar-core \
  --bundle ftar-vX.Y.Z.sigstore.json
```

`sha256sum -c SHA256SUMS.txt --ignore-missing` checks a download against the checksums (the checksum file is attested too).

## Security

Report security issues as public [issues](https://github.com/andy778/ftar-core/issues); see [SECURITY.md](SECURITY.md).
