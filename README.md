# ftar-core

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
- [ ] Compare numbers with reports from the original engine (needs a native OpenFTA build)
- [x] Failure statistics report, text report files compatible with OpenFTA
- [x] Writing `.fta`/`.ped`, tree editing (GUI: add/delete/move/retype/rename, event database, OpenPED-style primary event editor (Edit > Primary events) that also lists the other trees in the folder using an event (an ftar addition; MCP/automation `event_usage`), undo/redo, save, unsaved-changes prompt)
- [x] GUI (egui): tree canvas, editing, File/Edit menus, analyses, report and Monte Carlo failure-mode export
- [x] Image export (PNG/SVG) and printing: `ftar-render` draws the tree headlessly; GUI File > Export image / Print; `ftar draw`; MCP/automation `tree_export`

## Releases

Pushing a tag `vX.Y.Z` (it must equal the `ftar-cli` version in `Cargo.toml`) runs `.github/workflows/release.yml`:
it builds `ftar` and `ftar-gui` with `cargo build --release --locked` for Linux (x86_64), Windows (x86_64) and macOS
(arm64), smoke-tests the CLI, packages each platform (binaries, LICENSE, NOTICE, README, docs), and publishes a GitHub release with
the archives and `SHA256SUMS.txt`. The same workflow runs as a build-only dry run on manual dispatch and on pull requests that
touch `Cargo.toml`, `Cargo.lock` or the workflow.

## Security

Report vulnerabilities privately, not in public issues: see [SECURITY.md](SECURITY.md).
