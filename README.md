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
- [ ] Failure statistics report, text report files compatible with OpenFTA
- [x] Writing `.fta`/`.ped`, tree editing (GUI: add/delete/move/retype/rename, event database, undo/redo, save, unsaved-changes prompt)
- [x] GUI (egui): tree canvas, editing, File/Edit menus, analyses, report and Monte Carlo failure-mode export
- [x] Image export (PNG/SVG) and printing: `ftar-render` draws the tree headlessly; GUI File > Export image / Print; `ftar draw`; MCP/automation `tree_export`

## License

GPL-2.0-or-later (derived from OpenFTA, (c) 2005 FSC Limited).
