//! `ftar-mcp` - a [Model Context Protocol](https://modelcontextprotocol.io) server
//! exposing the `ftar-automation` tools to agents.
//!
//! * transport: newline-delimited JSON-RPC 2.0 over stdio ([`serve`])
//! * the protocol logic is [`Server::handle`]: JSON in, JSON out, no I/O, so it is
//!   testable in-process
//! * only the `tools` capability is offered; every automation tool is one MCP tool
//! * file access is whatever the [`Session`] allows - run it with a root (the CLI
//!   refuses to start `ftar mcp` without `--root` unless `--unrestricted` is given)
//! * it is lenient about the handshake: requests before `initialize` are served
//! * nothing is ever written to stdout except protocol messages

use std::io::{self, BufRead, Read, Write};

use ftar_automation::{tools, Session};
use serde_json::{json, Map, Value};

pub const SERVER_NAME: &str = "ftar";
/// Protocol revisions this server speaks, newest first.
pub const SUPPORTED_VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];
/// Longest accepted request line (bytes).
pub const MAX_LINE: usize = 8 * 1024 * 1024;

const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;

const INSTRUCTIONS: &str = "Fault tree analysis (OpenFTA-compatible). Open a tree with tree_open \
(raw=true to edit transfer trees), inspect it with tree_get / tree_validate, edit with tree_edit and \
event_set (undo with tree_undo), analyse with analysis_cutsets / analysis_probability / \
analysis_montecarlo, produce text reports with report, and write files with tree_save. Tools default \
to the most recently opened document; pass `doc` to pick another. Paths are relative to the server's root.";

pub struct Server {
    session: Session,
}

impl Server {
    pub fn new(session: Session) -> Self {
        Self { session }
    }

    /// Handle one parsed JSON-RPC message (a request, a notification, or a batch).
    /// Returns the message to send back, if any.
    pub fn handle(&mut self, msg: &Value) -> Option<Value> {
        match msg {
            Value::Array(items) => {
                if items.is_empty() {
                    return Some(error(Value::Null, INVALID_REQUEST, "empty batch"));
                }
                let out: Vec<Value> = items.iter().filter_map(|m| self.handle_one(m)).collect();
                (!out.is_empty()).then_some(Value::Array(out))
            }
            other => self.handle_one(other),
        }
    }

    /// Handle one line of input and return the line to write back, if any.
    pub fn handle_line(&mut self, line: &str) -> Option<String> {
        let line = line.trim();
        if line.is_empty() {
            return None;
        }
        let reply = match serde_json::from_str::<Value>(line) {
            Ok(v) => self.handle(&v),
            Err(e) => Some(error(
                Value::Null,
                PARSE_ERROR,
                &format!("parse error: {e}"),
            )),
        };
        reply.map(|v| v.to_string())
    }

    fn handle_one(&mut self, msg: &Value) -> Option<Value> {
        let Some(obj) = msg.as_object() else {
            return Some(error(
                Value::Null,
                INVALID_REQUEST,
                "request must be an object",
            ));
        };
        let id = obj.get("id").cloned();
        let method = obj.get("method").and_then(Value::as_str);
        let Some(method) = method else {
            // a response from the client (we never send requests) or garbage
            return match id {
                Some(id) if !obj.contains_key("result") && !obj.contains_key("error") => {
                    Some(error(id, INVALID_REQUEST, "missing `method`"))
                }
                _ => None,
            };
        };
        if obj.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
            return id.map(|id| error(id, INVALID_REQUEST, "`jsonrpc` must be \"2.0\""));
        }
        let params = obj.get("params").unwrap_or(&Value::Null);

        let Some(id) = id else {
            // notification: never answered (notifications/initialized, cancelled, ...)
            return None;
        };
        Some(match self.dispatch(method, params) {
            Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
            Err((code, message)) => error(id, code, &message),
        })
    }

    fn dispatch(&mut self, method: &str, params: &Value) -> Result<Value, (i64, String)> {
        match method {
            "initialize" => Ok(self.initialize(params)),
            "ping" => Ok(json!({})),
            "tools/list" => {
                Ok(json!({ "tools": tools().iter().map(tool_json).collect::<Vec<_>>() }))
            }
            "tools/call" => self.call_tool(params),
            _ => Err((METHOD_NOT_FOUND, format!("method not found: {method}"))),
        }
    }

    fn initialize(&self, params: &Value) -> Value {
        let asked = params.get("protocolVersion").and_then(Value::as_str);
        let version = asked
            .filter(|v| SUPPORTED_VERSIONS.contains(v))
            .unwrap_or(SUPPORTED_VERSIONS[0]);
        json!({
            "protocolVersion": version,
            "capabilities": { "tools": { "listChanged": false } },
            "serverInfo": { "name": SERVER_NAME, "version": env!("CARGO_PKG_VERSION") },
            "instructions": INSTRUCTIONS,
        })
    }

    fn call_tool(&mut self, params: &Value) -> Result<Value, (i64, String)> {
        let name = params.get("name").and_then(Value::as_str).ok_or((
            INVALID_PARAMS,
            "tools/call needs a string `name`".to_string(),
        ))?;
        if !tools().iter().any(|t| t.name == name) {
            return Err((INVALID_PARAMS, format!("unknown tool: {name}")));
        }
        let args = match params.get("arguments") {
            None | Some(Value::Null) => Value::Object(Map::new()),
            Some(v @ Value::Object(_)) => v.clone(),
            Some(_) => return Err((INVALID_PARAMS, "`arguments` must be an object".into())),
        };
        Ok(match self.session.call(name, &args) {
            Ok(v) => tool_result(&v, false),
            Err(e) => tool_result(&e.to_json(), true),
        })
    }
}

fn tool_result(v: &Value, is_error: bool) -> Value {
    let text = serde_json::to_string_pretty(v).unwrap_or_else(|_| v.to_string());
    json!({
        "content": [{ "type": "text", "text": text }],
        "structuredContent": v,
        "isError": is_error,
    })
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

/// Tools that only read state (hints for clients that ask for confirmation).
const READ_ONLY: &[&str] = &[
    "tree_list",
    "tree_info",
    "tree_get",
    "tree_validate",
    "event_list",
    "event_usage",
    "analysis_cutsets",
    "analysis_probability",
    "analysis_montecarlo",
    "report",
];

fn tool_json(t: &ftar_automation::ToolInfo) -> Value {
    let read_only = READ_ONLY.contains(&t.name);
    json!({
        "name": t.name,
        "description": t.description,
        "inputSchema": t.input_schema,
        "annotations": {
            "title": t.name,
            "readOnlyHint": read_only,
            // only these write files
            "destructiveHint": matches!(t.name, "tree_save" | "tree_export"),
            "idempotentHint": read_only,
            "openWorldHint": false,
        },
    })
}

/// Run the stdio loop: one JSON message per line in, one per line out.
/// Returns when the input ends. Over-long lines are rejected, not buffered.
pub fn serve<R: BufRead, W: Write>(
    server: &mut Server,
    mut input: R,
    mut out: W,
) -> io::Result<()> {
    let mut buf = Vec::new();
    loop {
        buf.clear();
        let n = input
            .by_ref()
            .take(MAX_LINE as u64 + 1)
            .read_until(b'\n', &mut buf)?;
        if n == 0 {
            return Ok(());
        }
        let reply = if buf.len() > MAX_LINE {
            // discard the rest of the line without keeping it
            let mut sink = Vec::new();
            loop {
                sink.clear();
                let m = input
                    .by_ref()
                    .take(64 * 1024)
                    .read_until(b'\n', &mut sink)?;
                if m == 0 || sink.ends_with(b"\n") {
                    break;
                }
            }
            Some(error(Value::Null, INVALID_REQUEST, "request line too long").to_string())
        } else {
            match std::str::from_utf8(&buf) {
                Ok(line) => server.handle_line(line),
                Err(_) => {
                    Some(error(Value::Null, PARSE_ERROR, "input is not valid UTF-8").to_string())
                }
            }
        };
        if let Some(r) = reply {
            out.write_all(r.as_bytes())?;
            out.write_all(b"\n")?;
            out.flush()?;
        }
    }
}
