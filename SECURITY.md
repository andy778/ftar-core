# Security Policy

This is a personal hobby project, provided **as is** with no warranty and no
guaranteed support, maintenance, or response. See the [LICENSE](LICENSE).

## Reporting a vulnerability

Found a security issue? Just open a public
[issue](https://github.com/andy778/ftar-core/issues). There is no private
disclosure process and no commitment to triage, respond, or fix within any
timeframe. Fixes, if any, land on `master` and ship in the next release.

## Scope and notes

- ftar treats every `.fta`, `.ped` and JSON script file as untrusted input
  (see [AGENTS.md](AGENTS.md) §3). A panic, hang or runaway memory use on such
  input is a bug worth reporting.
- `ftar mcp --root DIR` is meant to keep the client inside `DIR`.
  `ftar mcp --unrestricted` deliberately gives the client access to every path
  the user can reach.
- ftar's results can feed safety assessments. Check important results
  independently; the license disclaims any warranty.
- Release binaries carry SHA-256 checksums and a build provenance attestation;
  see "Verifying a download" in the [README](README.md).
- Dependencies are checked against the RustSec advisory database with
  `cargo deny` ([deny.toml](deny.toml)) on every PR and weekly.
