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
    // A new tree has no database name: saving names it t.ped and writes it,
    // so the event probabilities survive (this was silently lost before).
    let w = s.call("tree_save", &json!({ "path": "t.fta" })).unwrap();
    let written = w["written"].as_array().unwrap();
    assert_eq!(written.len(), 2, "{w}");
    assert!(written[1].as_str().unwrap().ends_with("t.ped"));
    assert!(!w.to_string().contains(r"\\\\?\\"), "{w}");
    assert!(w.get("warning").is_none());
    let re = s
        .call("tree_open", &json!({ "path": "t.fta", "raw": true }))
        .unwrap();
    assert_eq!(re["note"], "hello");
    assert_eq!(re["primary_events"], 2);
    assert_eq!(re["database"], "t.ped");
    assert_eq!(re["database_events"], 2);
    let p = s.call("analysis_probability", &json!({})).unwrap();
    assert!((p["top"].as_f64().unwrap() - 0.28).abs() < 1e-12);

    // ped=false writes only the tree, and says so
    let w = s
        .call("tree_save", &json!({ "path": "u.fta", "ped": false }))
        .unwrap();
    assert_eq!(w["written"].as_array().unwrap().len(), 1);
    assert!(w["warning"].is_string());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn closing_unsaved_work_needs_discard() {
    let mut s = session();
    s.call("tree_new", &json!({})).unwrap();
    // a fresh tree has nothing to lose
    let id = s.call("tree_new", &json!({})).unwrap()["doc"].clone();
    s.call(
        "tree_edit",
        &json!({ "doc": id, "op": "set_note", "note": "x" }),
    )
    .unwrap();
    let e = s.call("tree_close", &json!({ "doc": id })).unwrap_err();
    assert_eq!(e.code, "unsaved");
    assert_eq!(
        s.call("tree_list", &json!({})).unwrap()["documents"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    s.call("tree_close", &json!({ "doc": id, "discard": true }))
        .unwrap();
    s.call("tree_close", &json!({})).unwrap();
}

#[test]
fn raw_trees_with_transfers_explain_why_they_cannot_be_analysed() {
    let mut s = session();
    s.call("tree_open", &json!({ "path": "top.fta", "raw": true }))
        .unwrap();
    let v = s.call("tree_validate", &json!({})).unwrap();
    assert_eq!(v["analysable"], false, "{v}");
    assert!(v.to_string().contains("not inlined"), "{v}");
    for tool in [
        "analysis_cutsets",
        "analysis_probability",
        "analysis_montecarlo",
    ] {
        let e = s.call(tool, &json!({})).unwrap_err();
        assert_eq!(e.code, "raw_transfer", "{tool}: {e}");
        assert!(e.message.contains("without raw"), "{e}");
    }
    s.call("tree_open", &json!({ "path": "top.fta" })).unwrap();
    let v = s.call("tree_validate", &json!({})).unwrap();
    assert_eq!(v["analysable"], true);
    assert!(s.call("analysis_cutsets", &json!({})).is_ok());
}

#[test]
fn paths_outside_the_root_are_confinement_errors_even_if_missing() {
    let mut s = session();
    let mut bad = vec!["/etc/passwd", "/no/such/dir/x.fta"];
    // elsewhere this is a relative file name inside the root
    if cfg!(windows) {
        bad.push("C:\\no_such_dir_ftar\\x.fta");
    }
    for bad in bad {
        let e = s.call("tree_open", &json!({ "path": bad })).unwrap_err();
        assert_eq!(e.code, "confinement", "{bad}: {e}");
    }
    // a missing folder inside the root is still an i/o error
    let e = s
        .call("tree_open", &json!({ "path": "no_such_dir/x.fta" }))
        .unwrap_err();
    assert_eq!(e.code, "io", "{e}");
    assert!(!e.message.contains(r"\\?\"), "{e}");
}

#[test]
fn montecarlo_without_failures_reports_a_bound_not_a_zero_error() {
    let mut s = session();
    s.call("tree_new", &json!({})).unwrap();
    for id in ["a", "b"] {
        s.call(
            "tree_edit",
            &json!({ "op": "insert_child", "parent": [0], "kind": "basic", "id": id }),
        )
        .unwrap();
        s.call("event_set", &json!({ "id": id, "probability": 1e-9 }))
            .unwrap();
    }
    s.call(
        "tree_edit",
        &json!({ "op": "change_type", "path": [0], "kind": "and" }),
    )
    .unwrap();
    let r = s
        .call(
            "analysis_montecarlo",
            &json!({ "trials": 1000, "seed": 1, "method": "direct" }),
        )
        .unwrap();
    assert_eq!(r["system_failures"], 0, "{r}");
    assert!(r["std_error"].is_null(), "{r}");
    let b = r["upper_bound_95"].as_f64().unwrap();
    assert!((b - 3.0 / 1000.0).abs() < 1e-4, "{b}");
    let t = s
        .call(
            "report",
            &json!({ "kind": "montecarlo", "trials": 1000, "seed": 1, "method": "direct" }),
        )
        .unwrap();
    assert!(t["text"]
        .as_str()
        .unwrap()
        .contains("no failures simulated"));
    s.call("tree_close", &json!({ "discard": true })).unwrap();
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

/// A tree inside the root must not pull in its database or a transfer-in
/// tree from outside the root through the names stored in the file.
#[test]
fn files_named_inside_a_tree_stay_confined() {
    let base = std::env::temp_dir().join(format!("ftar_conf_named_{}", std::process::id()));
    let root = base.join("root");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(base.join("secret.ped"), "SECRET;g;B ;secret text;0.5; ;\n").unwrap();
    std::fs::write(
        base.join("sub.fta"),
        "NULL\nS NULL 0\n0\nM SUB 1\n6 Hidden\nO NULL 1\nB LEAKED 0\n",
    )
    .unwrap();
    std::fs::write(
        root.join("db.fta"),
        "../secret.ped\nS NULL 0\n0\nM TOP 1\n3 Top\nO NULL 1\nB SECRET 0\n",
    )
    .unwrap();
    std::fs::write(
        root.join("transfer.fta"),
        "NULL\nS NULL 0\n0\nM TOP 1\n3 Top\nO NULL 1\nI ../sub.fta 0\n",
    )
    .unwrap();

    let mut s = Session::with_root(&root).unwrap();
    for raw in [false, true] {
        s.call("tree_open", &json!({ "path": "db.fta", "raw": raw }))
            .unwrap();
        let ev = s.call("event_list", &json!({})).unwrap();
        assert!(!ev.to_string().contains("secret text"), "raw={raw}: {ev}");
    }
    let e = s
        .call("tree_open", &json!({ "path": "transfer.fta" }))
        .unwrap_err();
    assert_eq!(e.code, "io", "{e}");
    assert!(!e.message.contains("LEAKED"));

    // The same files load when the root is wide enough to contain them.
    let mut wide = Session::with_root(&base).unwrap();
    let ev = wide
        .call("tree_open", &json!({ "path": "root/transfer.fta" }))
        .and_then(|_| wide.call("analysis_cutsets", &json!({})))
        .unwrap();
    assert!(ev.to_string().contains("LEAKED"));
    let _ = std::fs::remove_dir_all(base);
}

/// A deeply nested file is rejected with an error instead of overflowing the stack.
#[test]
fn deeply_nested_tree_is_an_error_not_a_crash() {
    let dir = std::env::temp_dir().join(format!("ftar_deep_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut f = String::from("NULL\nS NULL 0\n0\n");
    f.push_str(&"O NULL 1\n".repeat(200_000));
    f.push_str("B X 0\n");
    std::fs::write(dir.join("deep.fta"), f).unwrap();
    let mut s = Session::with_root(&dir).unwrap();
    let e = s
        .call("tree_open", &json!({ "path": "deep.fta" }))
        .unwrap_err();
    assert!(e.message.contains("levels deep"), "{e}");
    let _ = std::fs::remove_dir_all(dir);
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

#[test]
fn tree_export_writes_png_svg_and_html_inside_the_sandbox() {
    let dir = std::env::temp_dir().join(format!("ftar_export_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut s = Session::with_root(&dir).unwrap();
    std::fs::copy(format!("{DATA}/theatre.fta"), dir.join("theatre.fta")).unwrap();
    std::fs::copy(format!("{DATA}/theatre.ped"), dir.join("theatre.ped")).unwrap();
    s.call("tree_open", &json!({ "path": "theatre.fta" }))
        .unwrap();

    let png = s
        .call("tree_export", &json!({ "path": "t.png", "scale": 1 }))
        .unwrap();
    assert_eq!(png["format"], "png");
    assert_eq!(&std::fs::read(dir.join("t.png")).unwrap()[..4], b"\x89PNG");
    let svg = s.call("tree_export", &json!({ "path": "t.svg" })).unwrap();
    assert!(svg["bytes"].as_u64().unwrap() > 500);
    assert!(std::fs::read_to_string(dir.join("t.svg"))
        .unwrap()
        .contains("RELAY_FAIL"));
    // explicit format overrides the extension
    s.call(
        "tree_export",
        &json!({ "path": "page.dat", "format": "html" }),
    )
    .unwrap();
    assert!(std::fs::read_to_string(dir.join("page.dat"))
        .unwrap()
        .contains("<!doctype html>"));

    for (args, code) in [
        (json!({ "path": "t.bmp" }), "invalid_args"),
        (json!({ "path": "t.png", "scale": 0 }), "invalid_args"),
        (json!({ "path": "../leak.png" }), "confinement"),
        (json!({}), "invalid_args"),
    ] {
        assert_eq!(
            s.call("tree_export", &args).unwrap_err().code,
            code,
            "{args}"
        );
    }
    assert!(!dir.parent().unwrap().join("leak.png").exists());
    let _ = std::fs::remove_dir_all(dir);
}

/// Trees in different folders share one `.ped` (the tester's pressure sensor
/// reused across projects): attaching reads the shared events in, saving
/// writes them back to the shared file, and nothing leaves the root.
#[test]
fn trees_in_different_folders_share_one_database() {
    let base = std::env::temp_dir().join(format!("ftar_shared_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let root = base.join("root");
    for sub in ["a", "b", "library"] {
        std::fs::create_dir_all(root.join(sub)).unwrap();
    }
    std::fs::copy(format!("{DATA}/lift.ped"), root.join("library/shared.ped")).unwrap();
    std::fs::copy(format!("{DATA}/lift.fta"), root.join("a/lift.fta")).unwrap();
    std::fs::copy(format!("{DATA}/lift.fta"), root.join("b/lift.fta")).unwrap();

    let mut s = Session::with_root(&root).unwrap();
    s.call("tree_open", &json!({ "path": "a/lift.fta" }))
        .unwrap();
    // a/ has no lift.ped, so the tree starts without event data
    let r = s
        .call(
            "tree_edit",
            &json!({ "op": "set_database", "name": "../library/shared.ped" }),
        )
        .unwrap();
    assert_eq!(r["database"], "../library/shared.ped");
    assert!(r["events_loaded"].as_u64().unwrap() >= 12, "{r}");
    s.call("event_set", &json!({ "id": "DOD_1", "probability": 0.25 }))
        .unwrap();
    let w = s.call("tree_save", &json!({})).unwrap();
    assert!(!root.join("a/shared.ped").exists(), "{w}");
    let shared = std::fs::read_to_string(root.join("library/shared.ped")).unwrap();
    assert!(shared.contains("DOD_1;"), "{shared}");
    assert!(shared.contains("CP_1;"), "other events kept: {shared}");

    // the other tree picks up the edited value from the shared file
    s.call("tree_open", &json!({ "path": "b/lift.fta" }))
        .unwrap();
    s.call(
        "tree_edit",
        &json!({ "op": "set_database", "name": "../library/shared.ped" }),
    )
    .unwrap();
    let list = s.call("event_list", &json!({})).unwrap();
    let dod = list["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == "DOD_1")
        .cloned()
        .unwrap();
    assert_eq!(dod["probability"], 0.25, "{dod}");

    // a database outside the root is neither read nor written
    s.call(
        "tree_edit",
        &json!({ "op": "set_database", "name": "../../outside.ped" }),
    )
    .unwrap();
    let e = s.call("tree_save", &json!({})).unwrap_err();
    assert_eq!(e.code, "confinement");
    assert!(!base.join("outside.ped").exists());
    let _ = std::fs::remove_dir_all(base);
}

#[test]
fn set_database_attaches_detaches_and_refuses_paths() {
    let mut s = session();
    s.call("tree_open", &json!({ "path": "lift.fta" })).unwrap();
    let r = s
        .call(
            "tree_edit",
            &json!({ "op": "set_database", "name": "sensors.ped" }),
        )
        .unwrap();
    assert_eq!(r["database"], "sensors.ped");
    assert_eq!(
        s.call("tree_info", &json!({})).unwrap()["database"],
        "sensors.ped"
    );
    // a name has to end in a file name
    let e = s
        .call(
            "tree_edit",
            &json!({ "op": "set_database", "name": "library/" }),
        )
        .unwrap_err();
    assert_eq!(e.code, "edit_rejected");
    s.call("tree_undo", &json!({})).unwrap();
    assert_eq!(
        s.call("tree_info", &json!({})).unwrap()["database"],
        "lift.ped"
    );
    let r = s
        .call("tree_edit", &json!({ "op": "set_database" }))
        .unwrap();
    assert!(r["database"].is_null());
}

/// OpenFTA prints `Tree   : lift.fta` (file name with extension) in report headers.
#[test]
fn report_header_names_the_tree_file_with_extension() {
    let mut s = session();
    s.call("tree_open", &json!({ "path": "lift.fta" })).unwrap();
    let r = s.call("report", &json!({ "kind": "probability" })).unwrap();
    assert!(
        r["text"]
            .as_str()
            .unwrap()
            .contains("\nTree   : lift.fta\n"),
        "{r}"
    );
}

#[test]
fn event_set_validates_sets_dormant_and_event_delete_undoes() {
    let mut s = session();
    s.call("tree_open", &json!({ "path": "lift.fta" })).unwrap();
    let r = s
        .call(
            "event_set",
            &json!({ "id": "DOD_1", "probability": 0.25, "dormant": true, "description": "Door sensor" }),
        )
        .unwrap();
    assert_eq!(r["dormant"], true);
    // a description with `;` would corrupt the .ped file
    let e = s
        .call(
            "event_set",
            &json!({ "id": "DOD_1", "probability": 0.25, "description": "a;b" }),
        )
        .unwrap_err();
    assert_eq!(e.code, "edit_rejected");
    let d = s.call("event_delete", &json!({ "id": "DOD_1" })).unwrap();
    assert!(d["still_used_by_nodes"].as_u64().unwrap() >= 1);
    assert!(s.call("event_delete", &json!({ "id": "DOD_1" })).is_err());
    s.call("tree_undo", &json!({})).unwrap();
    let list = s.call("event_list", &json!({})).unwrap();
    assert!(list["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["id"] == "DOD_1"));
}

/// Before changing a shared event, an agent can see which other trees use it.
#[test]
fn event_usage_lists_other_trees_on_the_same_database() {
    let base = std::env::temp_dir().join(format!("ftar_usage_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();
    for f in ["lift.fta", "lift.ped"] {
        std::fs::copy(format!("{DATA}/{f}"), base.join(f)).unwrap();
    }
    std::fs::copy(format!("{DATA}/lift.fta"), base.join("lift_b.fta")).unwrap();
    // same ids, another database: not a use of lift.ped's events
    let other = std::fs::read_to_string(base.join("lift.fta"))
        .unwrap()
        .replacen("lift.ped", "other.ped", 1);
    std::fs::write(base.join("other.fta"), other).unwrap();

    let mut s = Session::with_root(&base).unwrap();
    s.call("tree_open", &json!({ "path": "lift.fta", "raw": true }))
        .unwrap();
    let r = s.call("event_usage", &json!({ "id": "DOD_1" })).unwrap();
    assert_eq!(r["database"], "lift.ped", "{r}");
    assert_eq!(r["in_database"], true);
    assert!(r["this_tree_nodes"].as_u64().unwrap() >= 1, "{r}");
    let others = r["other_trees"].as_array().unwrap();
    assert_eq!(others.len(), 1, "{r}");
    assert!(others[0]["path"].as_str().unwrap().ends_with("lift_b.fta"));
    assert_eq!(others[0]["nodes"], r["this_tree_nodes"]);

    let none = s.call("event_usage", &json!({ "id": "NO_SUCH" })).unwrap();
    assert_eq!(none["this_tree_nodes"], 0);
    assert!(none["other_trees"].as_array().unwrap().is_empty());

    s.call("tree_new", &json!({})).unwrap();
    let e = s
        .call("event_usage", &json!({ "id": "DOD_1" }))
        .unwrap_err();
    assert_eq!(e.code, "invalid_args");
    let _ = std::fs::remove_dir_all(base);
}
