# Security policy

## Reporting a vulnerability

Please **do not** report security problems in public issues, pull requests or discussions.

Report them privately through GitHub:
[**Report a vulnerability**](https://github.com/andy778/ftar-core/security/advisories/new)
(repository *Security* tab → *Report a vulnerability*).

Include what you can of:

- the affected version (release tag, or commit if built from source) and platform;
- the input that triggers it (`.fta`, `.ped`, script or MCP request), or steps to reproduce;
- what happens (crash, hang, file access outside the root, wrong result, ...) and the impact you see.

ftar is maintained by one person in their spare time. The aim is to acknowledge a report within
7 days and to agree a fix and disclosure date with you. Fixes are released as a new version and
published as a GitHub security advisory, crediting you unless you prefer otherwise.

## Supported versions

Only the latest release gets security fixes. Fixes are not backported to older versions.

| Version | Supported |
|---|---|
| latest release | yes |
| older releases | no |

## Scope

ftar treats every input as untrusted (see [AGENTS.md](AGENTS.md) §3). In scope, for example:

- a panic, hang, unbounded memory use or crash caused by a `.fta`, `.ped` or JSON script file;
- the MCP server (`ftar mcp --root DIR`) reading or writing a file outside its root, or crashing on a
  request;
- a silently wrong analysis result caused by crafted input, where it could mislead a safety assessment;
- problems in the release process, such as release files that don't match their checksums or
  provenance attestation.

Out of scope:

- `ftar mcp --unrestricted`, which deliberately gives the client access to any path the user can reach;
- vulnerabilities in dependencies that ftar does not use in an exploitable way (please report those
  upstream; `cargo deny` checks the RustSec database weekly);
- differences from the original OpenFTA's numbers that are not security issues: open a normal bug report.
