# ftar architecture

Modelled on the layered, headless-first layout of PdfCraft (storytold/pdfcraft): everything is a library first, frontends are thin, and one command (`cargo xtask ci`) is the gate for humans, agents and CI.

## Workspace

| Path | Role |
|---|---|
| `crates/core` (`ftar-core`) | Model, `.fta`/`.ped` parse + write, tree editing with undo, minimal cut sets, exact probability, importance, Monte Carlo, failure statistics, text reports |
| `crates/render` (`ftar-render`) | Headless drawing of trees: SVG, PNG (resvg) and a printable HTML page; layout comes from `ftar-core::layout`, shared with the GUI canvas |
| `crates/automation` (`ftar-automation`) | Headless JSON tool catalogue over the core: the one API for scripts, agents and (opt-in) MCP |
| `crates/bundle` (`ftar-bundle`) | Zip bundles of a tree with its database, transfer-in trees and reports, for bug reports (the only crate besides render with external dependencies: `zip`) |
| `crates/mcp` (`ftar-mcp`) | Model Context Protocol server (stdio JSON-RPC) over the automation tools; see `docs/mcp.md` |
| `apps/ftar-cli` | `ftar` command line: analyses, `tools`/`call`/`run` for the automation tools, `mcp` for the MCP server |
| `apps/ftar-gui` | egui desktop app; testable headless with `egui_kittest` |
| `tools/openfta-native` | Headless build of the original OpenFTA C engine; makes the report fixtures ftar is compared against (not built in CI) |
| `xtask` | `cargo xtask ci|fmt|clippy|layers|test|deny` |

## Layers (enforced by `cargo xtask layers`)

L0 model/parse/write -> L1 analysis (cut sets, probability, Monte Carlo, reports) -> L2 automation tools -> L3 MCP server -> L4 frontends (CLI, GUI). Nothing below L3 may depend on egui, eframe, winit or rfd.

## Agentic workflow

- Any capability is first an automation tool (JSON in, JSON out, deterministic given a seed) so an agent can drive and verify it without a display.
- GUI behaviour is verified through kittest: load a tree, click, assert on the accessibility tree and the app state.
- CI runs `cargo xtask ci` on Linux and Windows. Agents run the same command locally, one worktree and one target dir each.
- Correctness is anchored by an independent brute-force oracle (`tests/verify.rs`) and notes in `docs/VERIFICATION.md`.
