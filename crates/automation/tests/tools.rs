//! Headless end-to-end tests of the JSON tool API.

use ftar_automation::{run_script, tools, Session};
use serde_json::{json, Value};

const DATA: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../core/tests/data");

fn session() -> Session {
    Session::with_root(DATA).expect("root")
}

#[test]
fn catalogue_is_complete_and_schemas_are_objects() {
    let t = tools();
    assert!(t.len() >= 15);
    for tool in &t {
        assert_eq!(tool.input_schema["type"], "object", "{}", tool.name);
        assert!(!tool.description.is_empty());
    }
    let names: Vec<_> = t.iter().map(|t| t.name).collect();
    for n in [
        "tree_open",
        "tree_edit",
        "analysis_probability",
        "analysis_montecarlo",
        "report",
    ] {
        assert!(names.contains(&n), "{n}");
    }
}

#[test]
fn every_catalogue_tool_is_dispatchable() {
    let mut s = session();
    for t in tools() {
        // must never be reported as unknown, whatever the arguments
        let e = s.call(t.name, &json!({})).err();
        assert!(e.is_none_or(|e| e.code != "unknown_tool"), "{}", t.name);
    }
}

#[test]
fn open_and_analyse_theatre() {
    let mut s = session();
    let doc = s
        .call("tree_open", &json!({ "path": "theatre.fta" }))
        .unwrap();
    assert_eq!(doc["primary_events"], 3);
    let cs = s.call("analysis_cutsets", &json!({})).unwrap();
    assert_eq!(cs["count"], 2);
    let p = s.call("analysis_probability", &json!({})).unwrap();
    assert!((p["top"].as_f64().unwrap() - 2.0643378107574753e-3).abs() < 1e-12);
    assert_eq!(p["exact"], true);
    let mc = s
        .call(
            "analysis_montecarlo",
            &json!({ "trials": 10000, "seed": 7 }),
        )
        .unwrap();
    let again = s
        .call(
            "analysis_montecarlo",
            &json!({ "trials": 10000, "seed": 7 }),
        )
        .unwrap();
    assert_eq!(mc, again, "seeded runs are deterministic");
}

#[test]
fn transfer_trees_open_expanded() {
    let mut s = session();
    s.call("tree_open", &json!({ "path": "top.fta" })).unwrap();
    let p = s.call("analysis_probability", &json!({})).unwrap();
    assert!((p["top"].as_f64().unwrap() - 0.28).abs() < 1e-12);
}

#[test]
fn root_confinement_blocks_escapes() {
    let mut s = session();
    for bad in [
        "../Cargo.toml",
        "..\\Cargo.toml",
        "/etc/passwd",
        "C:\\Windows\\win.ini",
    ] {
        let e = s.call("tree_open", &json!({ "path": bad })).unwrap_err();
        assert!(e.code == "confinement" || e.code == "io", "{bad}: {e}");
    }
}

#[test]
fn errors_are_structured_not_panics() {
    let mut s = session();
    assert_eq!(s.call("nope", &json!({})).unwrap_err().code, "unknown_tool");
    assert_eq!(
        s.call("analysis_cutsets", &json!({})).unwrap_err().code,
        "invalid_args"
    ); // nothing open
    assert_eq!(
        s.call("tree_open", &json!({ "path": 5 })).unwrap_err().code,
        "invalid_args"
    );
    assert_eq!(
        s.call("tree_open", &json!([1, 2])).unwrap_err().code,
        "invalid_args"
    );
    assert_eq!(
        s.call("tree_open", &json!({ "path": "missing.fta" }))
            .unwrap_err()
            .code,
        "io"
    );
    s.call("tree_open", &json!({ "path": "theatre.fta" }))
        .unwrap();
    assert_eq!(
        s.call("analysis_montecarlo", &json!({ "trials": 0 }))
            .unwrap_err()
            .code,
        "invalid_args"
    );
    assert_eq!(
        s.call("tree_get", &json!({ "path": [9, 9] }))
            .unwrap_err()
            .code,
        "not_found"
    );
}

#[test]
fn build_a_tree_by_edits_then_undo_and_save() {
    let dir = std::env::temp_dir().join(format!("ftar_auto_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut s = Session::with_root(&dir).unwrap();
    s.call("tree_new", &json!({ "name": "t" })).unwrap();
    // root M -> [OR]; add two basic events under the OR
    s.call(
        "tree_edit",
        &json!({ "op": "insert_child", "parent": [0], "kind": "basic", "id": "a" }),
    )
    .unwrap();
    s.call(
        "tree_edit",
        &json!({ "op": "insert_child", "parent": [0], "kind": "B", "id": "b" }),
    )
    .unwrap();
    s.call("event_set", &json!({ "id": "a", "probability": 0.1 }))
        .unwrap();
    s.call("event_set", &json!({ "id": "b", "probability": 0.2 }))
        .unwrap();
    assert_eq!(s.call("tree_validate", &json!({})).unwrap()["ok"], true);
    let p = s.call("analysis_probability", &json!({})).unwrap();
    assert!((p["top"].as_f64().unwrap() - 0.28).abs() < 1e-12);

    // history has four steps: insert a, insert b, event_set a, event_set b.
    // Three undos remove event b's values, event a's values and the second insert.
    for _ in 0..3 {
        s.call("tree_undo", &json!({})).unwrap();
    }
    assert_eq!(s.call("analysis_cutsets", &json!({})).unwrap()["count"], 1);
    for _ in 0..3 {
        s.call("tree_redo", &json!({})).unwrap();
    }
    assert_eq!(s.call("analysis_cutsets", &json!({})).unwrap()["count"], 2);
    let p = s.call("analysis_probability", &json!({})).unwrap();
    assert!((p["top"].as_f64().unwrap() - 0.28).abs() < 1e-12);

    // a rejected edit leaves the tree and history untouched
    let before = s.call("tree_get", &json!({})).unwrap();
    let e = s
        .call(
            "tree_edit",
            &json!({ "op": "insert_child", "parent": [0, 0], "kind": "basic", "id": "c" }),
        )
        .unwrap_err();
    assert_eq!(e.code, "edit_rejected");
    assert_eq!(s.call("tree_get", &json!({})).unwrap(), before);

    // save + reopen round trip
    s.call("tree_edit", &json!({ "op": "set_note", "note": "hello" }))
        .unwrap();
    let w = s.call("tree_save", &json!({ "path": "t.fta" })).unwrap();
    assert_eq!(w["written"].as_array().unwrap().len(), 1);
    let re = s
        .call("tree_open", &json!({ "path": "t.fta", "raw": true }))
        .unwrap();
    assert_eq!(re["note"], "hello");
    assert_eq!(re["primary_events"], 2);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn reports_are_deterministic_text() {
    let mut s = session();
    s.call("tree_open", &json!({ "path": "theatre.fta" }))
        .unwrap();
    for kind in ["cutsets", "probability", "montecarlo", "failure_statistics"] {
        let a = s
            .call(
                "report",
                &json!({ "kind": kind, "trials": 5000, "seed": 1 }),
            )
            .unwrap();
        let b = s
            .call(
                "report",
                &json!({ "kind": kind, "trials": 5000, "seed": 1 }),
            )
            .unwrap();
        assert_eq!(a, b, "{kind}");
        assert!(a["text"].as_str().unwrap().len() > 50, "{kind}");
    }
}

#[test]
fn scripts_stop_at_first_error_unless_told_otherwise() {
    let mut s = session();
    let script = json!({ "steps": [
        { "tool": "tree_open", "args": { "path": "lift.fta" } },
        { "tool": "analysis_cutsets" },
        { "tool": "bogus" },
        { "tool": "tree_info" },
    ]});
    let r = run_script(&mut s, &script);
    assert_eq!(r.len(), 3);
    assert!(r[0].outcome.is_ok() && r[1].outcome.is_ok() && r[2].outcome.is_err());
    assert_eq!(r[1].to_json()["result"]["count"], 12);

    let mut s = session();
    let mut lenient = script.clone();
    lenient["continue_on_error"] = Value::Bool(true);
    assert_eq!(run_script(&mut s, &lenient).len(), 4);
    assert_eq!(run_script(&mut s, &json!({})).len(), 1);
}

#[test]
fn confinement_is_exact_for_real_files_outside_the_root() {
    let base = std::env::temp_dir().join(format!("ftar_conf_{}", std::process::id()));
    let root = base.join("root");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::copy(format!("{DATA}/theatre.fta"), base.join("outside.fta")).unwrap();
    std::fs::copy(format!("{DATA}/theatre.fta"), root.join("inside.fta")).unwrap();

    let mut s = Session::with_root(&root).unwrap();
    assert!(s
        .call("tree_open", &json!({ "path": "inside.fta" }))
        .is_ok());
    let rel = s
        .call("tree_open", &json!({ "path": "../outside.fta" }))
        .unwrap_err();
    assert_eq!(rel.code, "confinement");
    let abs = base.join("outside.fta").display().to_string();
    let e = s.call("tree_open", &json!({ "path": abs })).unwrap_err();
    assert_eq!(e.code, "confinement");
    // writing outside is blocked too
    let e = s
        .call("tree_save", &json!({ "path": "../leak.fta" }))
        .unwrap_err();
    assert_eq!(e.code, "confinement");
    assert!(!base.join("leak.fta").exists());
    let _ = std::fs::remove_dir_all(base);
}

#[test]
fn reported_paths_have_no_windows_verbatim_prefix() {
    let mut s = session();
    let v = s
        .call("tree_open", &json!({ "path": "theatre.fta" }))
        .unwrap();
    assert!(!v["path"].as_str().unwrap().starts_with(r"\?\"));
}

fn event_ids(s: &mut Session) -> Vec<String> {
    let v = s.call("event_list", &json!({})).unwrap();
    v["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["id"].as_str().unwrap().to_string())
        .collect()
}

/// Regression: `event_set` used to change the database outside the undo history, so
/// undoing an edit left an orphan database entry behind (found testing the MCP server).
#[test]
fn undo_and_redo_cover_event_set_as_well_as_tree_edits() {
    let mut s = session();
    s.call("tree_open", &json!({ "path": "theatre.fta", "raw": true }))
        .unwrap();
    let base = event_ids(&mut s);
    let top = |s: &mut Session| {
        s.call("analysis_probability", &json!({})).unwrap()["top"]
            .as_f64()
            .unwrap()
    };
    let before = top(&mut s);

    s.call(
        "tree_edit",
        &json!({ "op": "insert_child", "parent": [0], "kind": "basic", "id": "SPARE" }),
    )
    .unwrap();
    s.call("event_set", &json!({ "id": "SPARE", "probability": 0.5 }))
        .unwrap();
    assert!(event_ids(&mut s).contains(&"SPARE".to_string()));
    assert!((top(&mut s) - before / 2.0).abs() < 1e-12 || top(&mut s) < before);

    // first undo reverts the event_set (the last change): the entry is gone, the node remains
    assert_eq!(s.call("tree_undo", &json!({})).unwrap()["applied"], true);
    assert_eq!(event_ids(&mut s), base);
    assert_eq!(
        s.call("tree_get", &json!({ "path": [0, 2] })).unwrap()["id"],
        "SPARE"
    );
    // second undo removes the node: back to the original tree and database
    assert_eq!(s.call("tree_undo", &json!({})).unwrap()["applied"], true);
    assert_eq!(event_ids(&mut s), base);
    assert!(s.call("tree_get", &json!({ "path": [0, 2] })).is_err());
    assert!((top(&mut s) - before).abs() < 1e-15);
    assert_eq!(s.call("tree_undo", &json!({})).unwrap()["applied"], false);

    // redo brings both back, in order
    s.call("tree_redo", &json!({})).unwrap();
    s.call("tree_redo", &json!({})).unwrap();
    assert!(event_ids(&mut s).contains(&"SPARE".to_string()));
    assert_eq!(
        s.call("tree_get", &json!({ "path": [0, 2] })).unwrap()["id"],
        "SPARE"
    );
}

#[test]
fn changing_an_existing_event_can_be_undone() {
    let mut s = session();
    s.call("tree_open", &json!({ "path": "theatre.fta" }))
        .unwrap();
    let p = |s: &mut Session| {
        s.call("analysis_probability", &json!({})).unwrap()["top"]
            .as_f64()
            .unwrap()
    };
    let before = p(&mut s);
    s.call(
        "event_set",
        &json!({ "id": "MAINS_FAIL", "probability": 0.9 }),
    )
    .unwrap();
    assert!(p(&mut s) > before * 10.0);
    assert_eq!(s.call("tree_info", &json!({})).unwrap()["can_undo"], true);
    s.call("tree_undo", &json!({})).unwrap();
    assert!((p(&mut s) - before).abs() < 1e-15);
}

#[test]
fn rejected_edits_do_not_pollute_history_or_database() {
    let mut s = session();
    s.call("tree_open", &json!({ "path": "theatre.fta", "raw": true }))
        .unwrap();
    let base = event_ids(&mut s);
    let e = s
        .call(
            "tree_edit",
            &json!({ "op": "insert_child", "parent": [0, 0], "kind": "basic", "id": "X" }),
        )
        .unwrap_err();
    assert_eq!(e.code, "edit_rejected");
    assert_eq!(event_ids(&mut s), base);
    assert_eq!(s.call("tree_info", &json!({})).unwrap()["can_undo"], false);
}
