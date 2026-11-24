//! Black-box tests of the `ftar` binary (what agents and CI scripts see).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::process::Command;

const DATA: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../crates/core/tests/data");

fn ftar(args: &[&str]) -> (bool, String) {
    let o = Command::new(env!("CARGO_BIN_EXE_ftar"))
        .args(args)
        .output()
        .expect("run ftar");
    (
        o.status.success(),
        String::from_utf8_lossy(&o.stdout).into_owned(),
    )
}

#[test]
fn prob_prints_exact_value() {
    let (ok, out) = ftar(&["prob", &format!("{DATA}/theatre.fta")]);
    assert!(ok);
    assert!(out.contains("Exact value : 2.0643378107574753E-3"), "{out}");
}

#[test]
fn tools_lists_json_catalogue() {
    let (ok, out) = ftar(&["tools"]);
    assert!(ok);
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert!(v
        .as_array()
        .unwrap()
        .iter()
        .any(|t| t["name"] == "analysis_cutsets"));
}

#[test]
fn call_returns_json_and_failure_exit_code() {
    let (ok, out) = ftar(&[
        "call",
        "tree_open",
        r#"{"path":"theatre.fta"}"#,
        "--root",
        DATA,
    ]);
    assert!(ok, "{out}");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&out).unwrap()["primary_events"],
        3
    );
    let (ok, out) = ftar(&[
        "call",
        "tree_open",
        r#"{"path":"../x.fta"}"#,
        "--root",
        DATA,
    ]);
    assert!(!ok);
    assert!(out.contains("confinement"), "{out}");
}

#[test]
fn run_script_file() {
    let dir = std::env::temp_dir().join(format!("ftar_cli_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let script = dir.join("s.json");
    std::fs::write(
        &script,
        r#"{"steps":[{"tool":"tree_open","args":{"path":"lift.fta"}},{"tool":"analysis_cutsets"}]}"#,
    )
    .unwrap();
    let (ok, out) = ftar(&["run", script.to_str().unwrap(), "--root", DATA]);
    assert!(ok, "{out}");
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v[1]["result"]["count"], 12);
    let _ = std::fs::remove_dir_all(dir);
}

// ------------------------------------------------------------------- mcp

mod mcp {
    use std::io::{BufRead, BufReader, Write};
    use std::process::{Command, Stdio};

    use serde_json::{json, Value};

    use super::DATA;

    fn spawn(args: &[&str]) -> std::process::Child {
        Command::new(env!("CARGO_BIN_EXE_ftar"))
            .arg("mcp")
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn ftar mcp")
    }

    #[test]
    fn refuses_to_start_without_a_root() {
        let o = Command::new(env!("CARGO_BIN_EXE_ftar"))
            .arg("mcp")
            .output()
            .unwrap();
        assert!(!o.status.success());
        assert!(String::from_utf8_lossy(&o.stderr).contains("--root"));
        assert!(
            o.stdout.is_empty(),
            "nothing on stdout but protocol messages"
        );
        let o = Command::new(env!("CARGO_BIN_EXE_ftar"))
            .args(["mcp", "--root", DATA, "--unrestricted"])
            .output()
            .unwrap();
        assert!(!o.status.success());
    }

    fn ask(
        stdin: &mut std::process::ChildStdin,
        out: &mut BufReader<std::process::ChildStdout>,
        msg: Value,
    ) -> Value {
        writeln!(stdin, "{msg}").unwrap();
        let mut line = String::new();
        out.read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap_or_else(|e| panic!("bad line {line:?}: {e}"))
    }

    #[test]
    fn full_conversation_over_real_stdio() {
        let mut child = spawn(&["--root", DATA]);
        let mut stdin = child.stdin.take().unwrap();
        let mut out = BufReader::new(child.stdout.take().unwrap());

        let init = ask(
            &mut stdin,
            &mut out,
            json!({"jsonrpc":"2.0","id":1,"method":"initialize",
            "params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"0"}}}),
        );
        assert_eq!(init["result"]["serverInfo"]["name"], "ftar");
        writeln!(
            stdin,
            r#"{{"jsonrpc":"2.0","method":"notifications/initialized"}}"#
        )
        .unwrap();

        let tools = ask(
            &mut stdin,
            &mut out,
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        );
        assert!(tools["result"]["tools"].as_array().unwrap().len() >= 15);

        ask(
            &mut stdin,
            &mut out,
            json!({"jsonrpc":"2.0","id":3,"method":"tools/call",
            "params":{"name":"tree_open","arguments":{"path":"theatre.fta"}}}),
        );
        let p = ask(
            &mut stdin,
            &mut out,
            json!({"jsonrpc":"2.0","id":4,"method":"tools/call",
            "params":{"name":"analysis_probability","arguments":{}}}),
        );
        assert_eq!(p["id"], 4);
        assert!(
            (p["result"]["structuredContent"]["top"].as_f64().unwrap() - 2.0643378107574753e-3)
                .abs()
                < 1e-12
        );

        let escape = ask(
            &mut stdin,
            &mut out,
            json!({"jsonrpc":"2.0","id":5,"method":"tools/call",
            "params":{"name":"tree_open","arguments":{"path":"../x.fta"}}}),
        );
        assert_eq!(escape["result"]["structuredContent"]["code"], "confinement");

        drop(stdin); // EOF ends the server
        assert!(child.wait().unwrap().success());
    }
}

#[test]
fn draw_writes_png_svg_and_html() {
    let dir = std::env::temp_dir().join(format!("ftar_draw_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let tree = format!("{DATA}/theatre.fta");
    for (name, magic) in [
        ("t.png", &b"\x89PNG"[..]),
        ("t.svg", b"<svg"),
        ("t.html", b"<!do"),
    ] {
        let out = dir.join(name);
        let (ok, _) = ftar(&["draw", &tree, out.to_str().unwrap(), "--scale", "1"]);
        assert!(ok, "{name}");
        assert!(std::fs::read(&out).unwrap().starts_with(magic), "{name}");
    }
    let (ok, _) = ftar(&["draw", &tree, dir.join("t.bmp").to_str().unwrap()]);
    assert!(!ok);
    let (ok, _) = ftar(&["draw", &tree]);
    assert!(!ok);
    let _ = std::fs::remove_dir_all(dir);
}

/// Regression: `ftar` with no arguments indexed `args[0]` and panicked.
#[test]
fn no_arguments_prints_usage_and_fails() {
    let o = Command::new(env!("CARGO_BIN_EXE_ftar"))
        .output()
        .expect("run ftar");
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(!o.status.success(), "{err}");
    assert_eq!(o.status.code(), Some(1), "{err}");
    assert!(!err.contains("panicked"), "{err}");
    assert!(err.contains("usage: ftar <command>"), "{err}");
}

#[test]
fn draw_takes_page_setup_options() {
    let dir = std::env::temp_dir().join(format!("ftar_draw_page_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let tree = format!("{DATA}/theatre.fta");
    let out = dir.join("p.html");
    let (ok, _) = ftar(&[
        "draw",
        &tree,
        out.to_str().unwrap(),
        "--page-size",
        "Legal",
        "--orientation",
        "portrait",
        "--fit",
        "split",
        "--page-scale",
        "150",
        "--margin",
        "10",
    ]);
    assert!(ok);
    let html = std::fs::read_to_string(&out).unwrap();
    assert!(html.contains("size: 215.9mm 355.6mm"));
    let png = dir.join("s.png");
    let (ok, _) = ftar(&[
        "draw",
        &tree,
        png.to_str().unwrap(),
        "--sheet",
        "1",
        "--scale",
        "0.5",
    ]);
    assert!(ok);
    assert!(std::fs::read(&png).unwrap().starts_with(b"\x89PNG"));
    for bad in [
        &["--page-size", "A9"][..],
        &["--orientation", "x"],
        &["--sheet", "99"],
        &["--sheet", "0"],
        &["--margin", "wide"],
    ] {
        let mut args = vec!["draw", &tree, png.to_str().unwrap()];
        args.extend_from_slice(bad);
        assert!(!ftar(&args).0, "{bad:?}");
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn draw_heads_the_drawing_with_the_tree_name() {
    let dir = std::env::temp_dir().join(format!("ftar_draw_title_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let out = dir.join("t.svg");
    let (ok, _) = ftar(&[
        "draw",
        &format!("{DATA}/theatre.fta"),
        out.to_str().unwrap(),
    ]);
    assert!(ok);
    let svg = std::fs::read_to_string(&out).unwrap();
    assert!(svg.contains("Tree: theatre"), "header with the tree name");
    let _ = std::fs::remove_dir_all(dir);
}
