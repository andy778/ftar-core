# ftar architecture

Modelled on the layered, headless-first layout of PdfCraft (storytold/pdfcraft): everything is a library first, frontends are thin, and one command (`cargo xtask ci`) is the gate for humans, agents and CI.

## Workspace

| Path | Role |
|---|---|
| `crates/core` (`ftar-core`) | Model, `.fta`/`.ped` parse + write, tree editing with undo, minimal cut sets, exact probability, importance, Monte Carlo, failure statistics, text reports |
| `crates/automation` (`ftar-automation`) | Headless JSON tool catalogue over the core: the one API for scripts, agents and (opt-in) MCP |
| `apps/ftar-cli` | `ftar` command line; subcommands call automation tools; `run --script steps.json` |
| `apps/ftar-gui` | egui desktop app; testable headless with `egui_kittest` |
| `xtask` | `cargo xtask ci|fmt|clippy|test|layers` |

## Layers (enforced by `cargo xtask layers`)

L0 model/parse/write -> L1 analysis (cut sets, probability, Monte Carlo, reports) -> L2 automation tools -> L3 frontends (CLI, GUI). Nothing below L3 may depend on egui, eframe, winit or rfd.

## Agentic workflow

- Any capability is first an automation tool (JSON in, JSON out, deterministic given a seed) so an agent can drive and verify it without a display.
- GUI behaviour is verified through kittest: load a tree, click, assert on the accessibility tree and the app state.
- CI runs `cargo xtask ci` on Linux and Windows. Agents run the same command locally, one worktree and one target dir each.
- Correctness is anchored by an independent brute-force oracle (`tests/verify.rs`) and notes in `docs/VERIFICATION.md`.
