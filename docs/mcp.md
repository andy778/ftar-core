# ftar MCP server

`ftar mcp` is a [Model Context Protocol](https://modelcontextprotocol.io) server on stdio. It exposes every
`ftar-automation` tool (open, inspect, edit, validate, analyse, report, save) to an agent. It is opt-in and
confined: it refuses to start without `--root DIR`, and every file path a tool reads or writes must stay inside
that directory (`..`, absolute paths outside it, and saving outside it are rejected). The same holds for files a
tree names itself: a database (`.ped`) outside the root is ignored, and a transfer-in tree outside it is an error.
Files over 64 MiB and trees nested more than 1000 levels deep are refused.

```
ftar mcp --root path/to/trees          # confined (recommended)
ftar mcp --unrestricted                # any path; only for trusted clients
```

## Register it with a client

Claude Code / any client that reads `.mcp.json`:

```json
{
  "mcpServers": {
    "ftar": {
      "command": "ftar",
      "args": ["mcp", "--root", "C:/path/to/trees"]
    }
  }
}
```

During development use `cargo run -q -p ftar-cli -- mcp --root DIR` as the command.

## Protocol

- Transport: newline-delimited JSON-RPC 2.0 on stdin/stdout; logs go to stderr only.
- Revisions: 2025-06-18, 2025-03-26, 2024-11-05 (the client's is echoed when supported).
- Capabilities: `tools` only. `tools/list` returns the catalogue with JSON Schemas and read-only / destructive hints;
  `tools/call` returns the result as pretty JSON in `content[0].text` and as `structuredContent`.
- A tool failure (bad arguments, rejected edit, confinement, analysis error) is a normal result with `isError: true`
  and `{code, message}`. Protocol errors (unknown tool, malformed request) are JSON-RPC errors.
- One session per server process: documents opened with `tree_open` stay open and are addressed by `doc`
  (default: the most recently opened).
- Lines over 8 MiB are rejected without buffering them.

## Typical agent flow

1. `tree_open {"path":"plant.fta"}` (use `"raw": true` if you will edit a tree with transfer gates; a raw tree with
   transfer-ins cannot be analysed, and `tree_validate` reports `"analysable": false`)
2. `tree_validate`, `tree_get`
3. `tree_edit` / `event_set` / `event_delete`, undo with `tree_undo`; `event_usage {"id":"PUMP"}` lists the other
   trees in the folder that use the event from the same `.ped` before you change it
4. `analysis_cutsets`, `analysis_probability`, `analysis_montecarlo` (seeded), `analysis_time` (q, intensity and rate over time with sampled uncertainty, as PFTA; `"importance": true` adds Birnbaum, criticality, Fussell–Vesely, RAW and RRW; docs/TEXT-FORMAT.md), or `report {"kind":"probability"}`.
   When a Monte Carlo run sees no failures, `std_error` is `null` and `upper_bound_95` gives the 95% upper bound.
5. `tree_save` writes the `.fta` and, when there are events, the `.ped` (a new tree gets `<name>.ped`). A path ending in `.ftt` writes the text format instead, with the tree and events in one file (docs/TEXT-FORMAT.md)
6. `tree_close` refuses a document with unsaved changes unless you pass `"discard": true`

## Testing

`cargo test -p ftar-mcp` exercises the protocol in-process; `cargo test -p ftar-cli` talks to a real `ftar mcp`
process over stdio. Both run in `cargo xtask ci`.
