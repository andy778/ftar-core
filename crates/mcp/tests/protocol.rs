//! In-process tests of the MCP protocol layer (no processes, no I/O).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use ftar_automation::Session;
use ftar_mcp::{serve, Server, MAX_LINE, SUPPORTED_VERSIONS};
use serde_json::{json, Value};

const DATA: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../core/tests/data");

fn server() -> Server {
    Server::new(Session::with_root(DATA).unwrap())
}

fn req(s: &mut Server, id: u64, method: &str, params: Value) -> Value {
    s.handle(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }))
        .expect("response")
}

fn call(s: &mut Server, name: &str, args: Value) -> Value {
    req(
        s,
        1,
        "tools/call",
        json!({ "name": name, "arguments": args }),
    )
}

#[test]
fn initialize_negotiates_version_and_advertises_tools_only() {
    let mut s = server();
    let r = req(
        &mut s,
        1,
        "initialize",
        json!({ "protocolVersion": "2025-03-26", "capabilities": {}, "clientInfo": {"name":"t","version":"0"} }),
    );
    assert_eq!(r["id"], 1);
    assert_eq!(r["result"]["protocolVersion"], "2025-03-26");
    assert_eq!(r["result"]["serverInfo"]["name"], "ftar");
    assert!(r["result"]["capabilities"]["tools"].is_object());
    assert!(r["result"]["capabilities"].get("resources").is_none());
    // unknown versions fall back to our newest
    let r = req(
        &mut s,
        2,
        "initialize",
        json!({ "protocolVersion": "1999-01-01" }),
    );
    assert_eq!(r["result"]["protocolVersion"], SUPPORTED_VERSIONS[0]);
}

#[test]
fn notifications_get_no_answer_and_ping_works() {
    let mut s = server();
    assert!(s
        .handle(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }))
        .is_none());
    assert_eq!(req(&mut s, 7, "ping", json!({}))["result"], json!({}));
}

#[test]
fn tools_list_mirrors_the_automation_catalogue() {
    let mut s = server();
    let r = req(&mut s, 1, "tools/list", json!({}));
    let list = r["result"]["tools"].as_array().unwrap();
    let catalogue = ftar_automation::tools();
    assert_eq!(list.len(), catalogue.len());
    for t in list {
        assert!(t["description"].as_str().is_some_and(|d| !d.is_empty()));
        assert_eq!(t["inputSchema"]["type"], "object");
    }
    let by = |n: &str| list.iter().find(|t| t["name"] == n).unwrap();
    assert_eq!(
        by("analysis_probability")["annotations"]["readOnlyHint"],
        true
    );
    assert_eq!(by("tree_edit")["annotations"]["readOnlyHint"], false);
    assert_eq!(by("tree_save")["annotations"]["destructiveHint"], true);
}

#[test]
fn calling_tools_returns_text_and_structured_content() {
    let mut s = server();
    let open = call(&mut s, "tree_open", json!({ "path": "theatre.fta" }));
    assert_eq!(open["result"]["isError"], false);
    assert_eq!(open["result"]["structuredContent"]["primary_events"], 3);
    let text = open["result"]["content"][0]["text"].as_str().unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(text).unwrap(),
        open["result"]["structuredContent"]
    );

    let p = call(&mut s, "analysis_probability", json!({}));
    let top = p["result"]["structuredContent"]["top"].as_f64().unwrap();
    assert!((top - 2.0643378107574753e-3).abs() < 1e-12);
}

#[test]
fn tool_failures_are_results_with_is_error_not_protocol_errors() {
    let mut s = server();
    let r = call(&mut s, "analysis_cutsets", json!({})); // nothing open
    assert!(r.get("error").is_none());
    assert_eq!(r["result"]["isError"], true);
    assert_eq!(r["result"]["structuredContent"]["code"], "invalid_args");
    let r = call(&mut s, "tree_open", json!({ "path": "../Cargo.toml" }));
    assert_eq!(r["result"]["isError"], true);
    assert_eq!(r["result"]["structuredContent"]["code"], "confinement");
}

#[test]
fn protocol_errors_have_the_right_codes() {
    let mut s = server();
    assert_eq!(
        req(&mut s, 1, "nope/nothing", json!({}))["error"]["code"],
        -32601
    );
    assert_eq!(
        call(&mut s, "no_such_tool", json!({}))["error"]["code"],
        -32602
    );
    assert_eq!(
        req(&mut s, 1, "tools/call", json!({}))["error"]["code"],
        -32602
    );
    assert_eq!(
        req(
            &mut s,
            1,
            "tools/call",
            json!({ "name": "tree_info", "arguments": [1] })
        )["error"]["code"],
        -32602
    );
    // missing/incorrect jsonrpc and bad shapes
    let r = s.handle(&json!({ "id": 3, "method": "ping" })).unwrap();
    assert_eq!(r["error"]["code"], -32600);
    assert_eq!(s.handle(&json!(5)).unwrap()["error"]["code"], -32600);
    assert_eq!(s.handle(&json!([])).unwrap()["error"]["code"], -32600);
    let r = s.handle_line("{not json").unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&r).unwrap()["error"]["code"],
        -32700
    );
}

#[test]
fn batches_answer_requests_only() {
    let mut s = server();
    let r = s
        .handle(&json!([
            { "jsonrpc": "2.0", "id": 1, "method": "ping" },
            { "jsonrpc": "2.0", "method": "notifications/initialized" },
            { "jsonrpc": "2.0", "id": 2, "method": "tools/list" }
        ]))
        .unwrap();
    let a = r.as_array().unwrap();
    assert_eq!(a.len(), 2);
    assert_eq!(
        (a[0]["id"].clone(), a[1]["id"].clone()),
        (json!(1), json!(2))
    );
    assert!(s
        .handle(&json!([{ "jsonrpc": "2.0", "method": "notifications/initialized" }]))
        .is_none());
}

#[test]
fn state_is_kept_across_calls_in_one_session() {
    let mut s = server();
    call(
        &mut s,
        "tree_open",
        json!({ "path": "lift.fta", "raw": true }),
    );
    let e = call(
        &mut s,
        "tree_edit",
        json!({ "op": "set_note", "note": "n" }),
    );
    assert_eq!(e["result"]["isError"], false);
    let i = call(&mut s, "tree_info", json!({}));
    assert_eq!(i["result"]["structuredContent"]["dirty"], true);
    let u = call(&mut s, "tree_undo", json!({}));
    assert_eq!(u["result"]["structuredContent"]["applied"], true);
}

#[test]
fn stdio_loop_is_line_based_and_survives_garbage() {
    let input = [
        r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#,
        "this is not json",
        "",
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
    ]
    .join("\n");
    let mut out = Vec::new();
    serve(&mut server(), input.as_bytes(), &mut out).unwrap();
    let lines: Vec<Value> = String::from_utf8(out)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(lines.len(), 3);
    assert_eq!(lines[0]["id"], 1);
    assert_eq!(lines[1]["error"]["code"], -32700);
    assert!(lines[2]["result"]["tools"].is_array());
}

#[test]
fn oversized_lines_are_rejected_and_the_next_request_still_works() {
    let mut input = Vec::new();
    input.extend(std::iter::repeat_n(b'a', MAX_LINE + 10));
    input.push(b'\n');
    input.extend(br#"{"jsonrpc":"2.0","id":9,"method":"ping"}"#);
    input.push(b'\n');
    let mut out = Vec::new();
    serve(&mut server(), input.as_slice(), &mut out).unwrap();
    let lines: Vec<Value> = String::from_utf8(out)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert_eq!(lines[0]["error"]["code"], -32600);
    assert_eq!(lines[1]["id"], 9);
}
