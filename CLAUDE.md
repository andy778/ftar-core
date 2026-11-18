# ftar - instructions for agents

ftar is a Rust port of OpenFTA, an unmaintained fault tree analysis tool (Java GUI + C++ core). Read `AGENTS.md` first; it wins over this file.

## Orient
1. `README.md` (status checklist), `docs/architecture.md` (layers, crates), `docs/VERIFICATION.md` (what is proven equal to the original).
2. The reference implementation is at https://github.com/andy778/OpenFTA (branch `dev`): C core in `src/lib/{Analysis,Appn,PrimaryEvents}`, Java GUI in `src/gui/FTAGUI`. It cannot be built on this machine; read it.

## Run and look
- Headless CLI: `cargo run -p ftar-cli -- prob tests/data/theatre.fta` (also `cutsets`, `mc`, `info`, `run` for JSON scripts).
- GUI: `cargo run -p ftar-gui -- tests/data/theatre.fta`.
- Headless GUI test: `cargo test -p ftar-gui` (egui_kittest, no window or GPU).
- MCP server: `cargo run -q -p ftar-cli -- mcp --root DIR` (docs/mcp.md); register it in `.mcp.json` to let an agent drive ftar.
- All gates: `cargo xtask ci`.

## Conventions
- Rust edition 2021, `unsafe_code = "forbid"`, workspace lints, no JS/TS.
- Scope builds/tests to the crates you touched while iterating (`cargo test -p ftar-core`); run the full gate before committing.
- Test data in `tests/data/` are OpenFTA's own examples (GPL).
- Update the README status checklist when a capability lands.
