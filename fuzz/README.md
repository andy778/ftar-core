# Fuzzing

Every `.fta`, `.ped`, script and request is untrusted input and must never panic
(AGENTS.md §3). These [cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz) targets check that.

| Target | Input |
|---|---|
| `parse_fta` | `.fta` bytes; also round-trips through `write_fta` |
| `parse_ped` | `.ped` bytes; also round-trips through `write_ped` |
| `parse_text` | `.ftt` text-format bytes; what parses must write back and read back the same |
| `analysis` | `<.fta> NUL <.ped>`; minimal cut sets (order ≤ 4) and top probability |
| `mcp` | one JSON-RPC line to the MCP server, file access confined to a scratch dir |

Run locally (needs a nightly toolchain):

```sh
cargo install cargo-fuzz
cargo +nightly fuzz run parse_fta fuzz/corpus/parse_fta -- -max_total_time=60
```

Seed corpora are in `fuzz/corpus/<target>` (from `crates/core/tests/data`).
CI runs short sessions with ClusterFuzzLite (`.clusterfuzzlite/`,
`.github/workflows/cflite.yml`). Every crash it finds gets a regression test in
the affected crate (AGENTS.md §3).
