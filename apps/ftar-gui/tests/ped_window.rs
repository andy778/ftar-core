//! The primary event database window (issue #58): works without a tree,
//! saves/opens `.ped`, exports text, copies events and links tree events.

use std::path::PathBuf;

use eframe::egui::vec2;
use egui_kittest::{kittest::Queryable, Harness};
use ftar_core::EventType;
use ftar_gui::App;

fn data(name: &str) -> PathBuf {
    PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../crates/core/tests/data"
    ))
    .join(name)
}

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("ftar_ped_{}_{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn fill(app: &mut App, id: &str, prob: &str, kind: EventType) {
    app.ped.form.id = id.into();
    app.ped.form.prob = prob.into();
    app.ped.form.kind = kind;
    app.ped.form.desc = format!("event {id}");
    assert!(app.ped_apply(), "{:?}", app.ped.error);
}

#[test]
fn database_window_new_edit_save_open_export_without_a_tree() {
    let dir = scratch("roundtrip");
    let mut app = App::default();
    assert!(app.doc.is_none());
    app.ped_new();
    fill(&mut app, "PUMP", "1e-3", EventType::Basic);
    fill(&mut app, "VALVE", "0.25", EventType::External);
    assert!(app.ped.dirty);
    assert!(!app.ped_save(), "no name yet");
    let file = dir.join("plant.ped");
    assert!(app.ped_save_as(&file));
    assert!(!app.ped.dirty);

    let mut other = App::default();
    assert!(other.ped_open(&file));
    assert_eq!(other.ped.event_ids(), vec!["PUMP", "VALVE"]);
    other.ped_select("VALVE");
    assert_eq!(other.ped.form.kind, EventType::External);
    assert!(other.ped_delete());
    assert!(other.ped.dirty);
    assert!(other.ped_save());
    assert_eq!(
        std::fs::read(&file)
            .unwrap()
            .iter()
            .filter(|b| **b == b'\n')
            .count(),
        1
    );

    let txt = dir.join("plant.txt");
    assert!(other.ped_export_text(&txt));
    let t = std::fs::read_to_string(&txt).unwrap();
    assert!(t.contains("Database Filename: plant.ped"));
    assert!(t.contains("ID: PUMP\nProbability: 0.001\nType: Basic"));

    // bad input never panics
    let junk = dir.join("junk.ped");
    std::fs::write(&junk, b"\xff\x00;;;;\n;;\n").unwrap();
    let _ = other.ped_open(&junk);
    assert!(!other.ped_open(&dir.join("missing.ped")));
    assert!(other.ped.error.is_some());
    other.ped.form.prob = "abc".into();
    assert!(!other.ped_apply());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn copy_between_tree_and_database_window_and_link() {
    let mut app = App::default();
    app.open(data("lift.fta"));
    let tree_events = app.doc.as_ref().unwrap().db.events.len();
    assert!(tree_events > 0);
    app.ped_new();
    let out = app.ped_copy_from_tree(true).unwrap();
    assert_eq!(out.copied.len(), tree_events);
    assert_eq!(app.ped.db.events.len(), tree_events);

    fill(&mut app, "FRESH", "0.5", EventType::External);
    app.ped.selected = Some("FRESH".into());
    let out = app.ped_copy_to_tree(true).unwrap();
    assert_eq!(out.copied, vec!["FRESH".to_string()]);
    assert!(app.doc.as_ref().unwrap().db.events.contains_key("FRESH"));
    assert!(app.history.can_undo());

    // link a basic leaf to the external event: asks first
    let leaf = ftar_core::edit::all_paths(&app.doc.as_ref().unwrap().tree)
        .into_iter()
        .find(|p| {
            ftar_core::edit::node_at(&app.doc.as_ref().unwrap().tree, p)
                .is_some_and(|n| n.children.is_empty() && n.kind == EventType::Basic)
        })
        .unwrap();
    app.select(Some(leaf));
    assert_eq!(
        app.ped_link(),
        Some(ftar_core::database::LinkOutcome::TypesDiffer)
    );
    assert_eq!(app.ped.confirm_link.as_deref(), Some("FRESH"));
    assert!(app.ped_link_confirmed().is_some());
    let n = app.selected_node().unwrap();
    assert_eq!((n.id.as_str(), n.kind), ("FRESH", EventType::External));

    // no selection: an error, not a panic
    app.select(None);
    assert!(app.ped_link().is_none());
    assert!(app.ped.error.is_some());
}

#[test]
fn database_window_has_named_controls() {
    let mut app = App::default();
    app.ped_new();
    app.ped.open = true;
    fill(&mut app, "PUMP", "1e-3", EventType::Basic);
    let mut h = Harness::builder()
        .with_size(vec2(1280.0, 800.0))
        .build_eframe(move |_| app);
    h.run();
    for label in [
        "New",
        "Open...",
        "Save",
        "Save As...",
        "Export Text File...",
        "Event id",
        "Event probability",
        "Event description",
        "Apply event",
        "Delete event",
        "Copy to tree database",
        "Copy from tree database",
        "Link tree event",
        "PUMP",
    ] {
        assert!(h.get_all_by_label(label).count() >= 1, "{label}");
    }
    h.get_by_label("PUMP").click();
    h.run();
    assert_eq!(h.state().ped.form.id, "PUMP");
}
