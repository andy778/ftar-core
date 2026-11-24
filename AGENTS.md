# AGENTS.md - rules for every agent and contributor

These rules bind every AI agent and human contributor. `CLAUDE.md` holds day-to-day instructions; if they disagree, this file wins.

## 1. Provenance and licence
- ftar is a Rust port of OpenFTA (GPL-2.0-or-later, (c) 2005 FSC Limited). Keep that attribution in README, LICENSE and the crate metadata. Never remove it.
- Behaviour comes from the OpenFTA sources (C in `src/lib`, branch `dev` of andy778/OpenFTA), its manual, and tests. Do not copy code from non-GPL-compatible projects.
- No binary assets (icons, fonts, images) without a licence and an entry in `ATTRIBUTION.toml`.

## 2. Layering (enforced by `cargo xtask layers`)
- `crates/*` are headless libraries. They must never depend on egui, eframe, winit, rfd or any window/dialog crate.
- `apps/*` are thin frontends (CLI, GUI). Logic lives in crates; an app only wires UI to it.
- Every user-visible capability must exist as a headless function/tool first, then in the GUI. If you can only test it by clicking, it is not done.

## 3. Never panic
Every `.fta`, `.ped`, script and keystroke is untrusted input. In non-test code: no `unwrap`/`expect`/`panic!` unless provably infallible (comment why), no indexing with input-derived positions, bounded recursion and allocation, no `unsafe`. Return `Result` with the crate's `Error`. Every crash fix gets a regression test.

## 4. Tests are the contract
- Properties and a brute-force oracle (`tests/verify.rs`) beat hand-written expectations.
- Compare against the original engine's behaviour; record discrepancies in `docs/VERIFICATION.md`.
- GUI changes need a headless test (`egui_kittest`) in `apps/ftar-gui/tests`. Widgets must have accessible names; the harness fails otherwise.
- Prove a new test fails without the fix.

## 5. Gates before every commit
`cargo xtask ci` = `fmt --check`, `clippy -D warnings`, `layers`, `test` (cargo-nextest when installed, plus doctests), `deny` (bans, licences, sources; skipped locally without cargo-deny). CI runs the same command on Linux and Windows, and nightly on macOS. Do not weaken a gate to get green.

## 6. Parallel agents
Use one git worktree and one `CARGO_TARGET_DIR` per agent. Touch only the crate/files your task names; add one `pub mod` line per new module. Commit to your branch; the coordinating agent merges. Never push `master` directly from a worktree agent.

## 7. Commits
Small, green, one topic per commit. Branch `<type>/<slug>`; PR into `master`; merge when CI is green.
