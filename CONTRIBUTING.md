# Contributing to ftar

Thanks for helping. ftar is a Rust port of [OpenFTA](https://github.com/andy778/OpenFTA), a fault tree
analysis tool. Results from a fault tree tool feed safety decisions, so the bar is: every behaviour is
tested, compared with the original engine where possible, and never panics on bad input.

The binding rules for humans and AI agents are in [AGENTS.md](AGENTS.md); this page is the short version.
If the two disagree, AGENTS.md wins.

## Reporting bugs and asking for features

- Search the [issues](https://github.com/andy778/ftar-core/issues) first.
- Use the issue templates. For a wrong result or a crash, attach the `.fta`/`.ped` files (or the smallest
  tree that shows it), the command or GUI steps, the ftar version (the release tag, or the commit if built from source)
  and, if you have it, what OpenFTA reports for the same tree.
- Security problems are reported as ordinary public issues too; see [SECURITY.md](SECURITY.md).

## Every change starts with an issue

Release notes are built from issues and pull requests, so each fix or feature needs an issue:

- Open (or find) the issue first and label it `bug` or `enhancement`.
- Commit subjects end with the issue number: `Pad report columns like C printf (#9)`.
- The PR body says `Closes #N` when it completes the issue, or `Refs #N` when it only covers part of it.
  Don't write "close/fix/resolve #N" anywhere else in the body; GitHub would close the issue.

## Development setup

You need a stable Rust toolchain (see `rust-version` in [Cargo.toml](Cargo.toml)). On Linux, the GUI
needs: `libxkbcommon-dev libwayland-dev libgtk-3-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev`.

```sh
cargo run -p ftar-cli -- prob crates/core/tests/data/theatre.fta   # CLI
cargo run -p ftar-gui -- crates/core/tests/data/theatre.fta        # GUI
cargo test -p ftar-core                                            # one crate while iterating
cargo xtask ci                                                     # all gates, before every commit
```

Orientation: [docs/architecture.md](docs/architecture.md) (layers and crates),
[docs/VERIFICATION.md](docs/VERIFICATION.md) (what is proven equal to OpenFTA), [docs/mcp.md](docs/mcp.md)
(automation and the MCP server). Fuzzing is described in [fuzz/README.md](fuzz/README.md).

The Python helper scripts (`tools/bench`) are linted and formatted with ruff (rules, including
complexity limits, in [ruff.toml](ruff.toml)); CI runs `.github/workflows/ruff.yml` when they change:

```sh
uvx ruff check .     # lint
uvx ruff format .    # format
```

### Test quality: mutation testing and coverage

Passing tests only help if they would fail on a bug. Two optional tools check that (not part of
`cargo xtask ci`):

```sh
cargo xtask mutants      # mutation-test the Rust lines changed since origin/master (cargo install cargo-mutants --locked)
cargo xtask coverage     # line coverage as HTML (cargo install cargo-llvm-cov --locked; rustup component add llvm-tools-preview)
```

`cargo xtask mutants` makes small deliberate bugs in the changed code (flip a `<`, return a default)
and runs the tests against each. A **missed** mutant means no test noticed: add or tighten a test.
CI runs the same on every pull request (`.github/workflows/mutants.yml`, fails on missed mutants in
the PR's changes) and over the whole workspace weekly (a report in the job summary and artifacts). The weekly run on
`master` also publishes the **mutation score** (caught / (caught + missed + timed out)) for the README
badge, as `mutants.json` on the `badges` branch.
Coverage runs on every pull request and push to `master` (`.github/workflows/coverage.yml`); the
per-file table is in the job summary, and the total is on Codecov (README badge). It has no threshold: use it to find untested code, and
mutation testing to check the tested code is really checked.

## Rules that reviews check

- **Layering.** `crates/*` are headless libraries and never depend on egui, eframe, winit, rfd or any
  window/dialog crate; `apps/*` only wire UI to them. A capability exists as a headless function or tool
  first, then in the GUI. `cargo xtask layers` enforces this.
- **Never panic.** Every `.fta`, `.ped`, script and keystroke is untrusted. No `unwrap`/`expect`/`panic!`
  in non-test code unless provably infallible (with a comment saying why), no indexing with
  input-derived positions, bounded recursion and allocation, no `unsafe` (`unsafe_code = "forbid"`).
  Return the crate's `Error`.
- **Tests are the contract.** Prefer properties and the brute-force oracle (`crates/core/tests/verify.rs`) to
  hand-written expectations. Compare against OpenFTA's behaviour and record differences in
  `docs/VERIFICATION.md`. Show that a new test fails without the fix. Every crash fix gets a regression test.
- **GUI changes** need a headless `egui_kittest` test in `apps/ftar-gui/tests`, and widgets need
  accessible names.
- **Provenance and licence.** ftar is GPL-2.0-or-later and keeps the OpenFTA attribution. Don't copy
  code from GPL-incompatible projects. Binary assets need a licence and an entry in `ATTRIBUTION.toml`.
  New dependencies must pass `cargo deny check` (licence allow-list in [deny.toml](deny.toml)).
- **Docs.** Tick the README status checklist when a capability lands.

## Branches, commits and pull requests

- Branch from `master` as `<type>/<slug>`, e.g. `fix/report-wrapping`, `feat/event-usage`, `ci/codeql`,
  `docs/contributing`.
- Small commits, one topic each, each one green (`cargo xtask ci`). Commits to `master` must be signed;
  squash or merge commits made by GitHub are signed for you.
- Changes to `.github/workflows` go in their own PR, never mixed into a feature PR.
- Open a PR into `master` and fill in the template. Required checks: `cargo xtask ci` on Linux and
  Windows, `cargo deny`, CodeQL and ClusterFuzzLite. All review threads must be resolved before merging.

## Code of conduct

Everyone taking part is expected to follow the [Code of Conduct](CODE_OF_CONDUCT.md).

## Licence

By contributing you agree that your contribution is licensed under the project's licence,
GPL-2.0-or-later (see [LICENSE](LICENSE) and [NOTICE](NOTICE)).
