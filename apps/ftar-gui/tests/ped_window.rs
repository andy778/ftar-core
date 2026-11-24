//! The primary event database window (issue #58): works without a tree,
//! saves/opens `.ped`, exports text, copies events and links tree events.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use eframe::egui::vec2;
use egui_kittest::{
    kittest::{NodeT, Queryable},
    Harness,
};
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
        "Print...",
        "Id",
        "Probability",
        "Description",
        "Apply event",
        "Delete event",
        "Copy to tree database",
        "Copy from tree database",
        "Link tree event",
        "Primary Events",
        "Constant Failure Rate/Unit Time",
        "Dormant",
        "Type",
        "PUMP",
    ] {
        assert!(h.get_all_by_label(label).count() >= 1, "{label}");
    }
    h.get_by_label("PUMP").click();
    h.run();
    assert_eq!(h.state().ped.form.id, "PUMP");
}

// ------------------------------------------------ Edit > Link in the main menu (#70)

fn lift_with_database_event(kind: EventType) -> (App, Vec<usize>) {
    let mut app = App::default();
    app.open(data("lift.fta"));
    app.ped_new();
    fill(&mut app, "FRESH", "0.5", kind);
    app.ped.selected = Some("FRESH".into());
    let leaf = ftar_core::edit::all_paths(&app.doc.as_ref().unwrap().tree)
        .into_iter()
        .find(|p| {
            ftar_core::edit::node_at(&app.doc.as_ref().unwrap().tree, p)
                .is_some_and(|n| n.children.is_empty() && n.kind == EventType::Basic)
        })
        .unwrap();
    (app, leaf)
}

#[test]
fn edit_link_needs_a_tree_event_and_a_database_event() {
    let (mut app, leaf) = lift_with_database_event(EventType::Basic);
    assert!(!app.can_link(), "no tree event selected yet");
    app.select(Some(leaf.clone()));
    assert!(app.can_link());
    app.ped.selected = None;
    assert!(!app.can_link(), "no database event selected");
    app.ped.selected = Some("FRESH".into());
    app.select(Some(vec![]));
    assert!(!app.can_link(), "the top event is not a primary event");
    // linking reports problems in the main status line too
    app.edit_link();
    assert!(app.error.is_some());
}

#[test]
fn edit_link_links_and_can_be_undone_without_the_database_window() {
    let (mut app, leaf) = lift_with_database_event(EventType::Basic);
    app.ped.open = false;
    app.select(Some(leaf.clone()));
    app.edit_link();
    assert!(app.error.is_none(), "{:?}", app.error);
    assert_eq!(app.selected_node().unwrap().id, "FRESH");
    assert!(app.status.as_deref().unwrap().contains("Linked to FRESH"));
    app.undo();
    assert_ne!(app.selected_node().unwrap().id, "FRESH");
}

#[test]
fn edit_menu_link_asks_when_types_differ_even_with_the_window_closed() {
    let (mut app, leaf) = lift_with_database_event(EventType::External);
    app.ped.open = false;
    app.select(Some(leaf));
    let mut h = Harness::builder()
        .with_size(vec2(1280.0, 800.0))
        .build_eframe(move |_| app);
    h.run();
    h.get_by_label("Edit").click();
    h.run();
    let link = h
        .query_all_by_label_contains("Link")
        .find(|n| {
            n.accesskit_node()
                .label()
                .is_some_and(|l| l.starts_with("Link"))
        })
        .expect("Edit > Link");
    link.click();
    h.run();
    assert_eq!(h.state().ped.confirm_link.as_deref(), Some("FRESH"));
    h.get_by_label_contains("have different types");
    h.get_by_label("Link anyway").click();
    h.run();
    assert_eq!(h.state().selected_node().unwrap().id, "FRESH");
    assert!(h.state().ped.confirm_link.is_none());
}

// ------------------------------------------------ #87 radios, #88 Clear, #89 Help

#[test]
fn database_window_has_type_radios_clear_and_help() {
    let mut app = App::default();
    app.ped_new();
    app.ped.open = true;
    fill(&mut app, "PUMP", "1e-3", EventType::Basic);
    let mut h = Harness::builder()
        .with_size(vec2(1280.0, 800.0))
        .build_eframe(move |_| app);
    h.run();
    for label in [
        "Basic",
        "External",
        "Undeveloped",
        "Conditional (not analysed)",
        "Conditional (analysed)",
    ] {
        h.get_by_label(label);
    }
    h.get_by_label("Undeveloped").click();
    h.run();
    assert_eq!(h.state().ped.form.kind, EventType::Undeveloped);

    h.get_by_label("Clear").click();
    h.run();
    assert!(h.state().ped.selected.is_none());
    assert!(h.state().ped.form.id.is_empty());
    assert_eq!(
        h.state().ped.event_ids(),
        vec!["PUMP"],
        "database unchanged"
    );

    h.get_all_by_label("Help").last().unwrap().click(); // the menu bar has a Help too
    h.run();
    assert!(h.state().help.open);
    let t = h.state().help.topic;
    assert_eq!(h.state().help.topics[t].title, "Primary events");
}

// ------------------------------------------------ #93 menu bar, toolbar, print

#[test]
fn database_window_menus_and_print() {
    let dir = scratch("menus");
    let mut app = App::default();
    app.ped_new();
    app.ped.open = true;
    fill(&mut app, "PUMP", "1e-3", EventType::Basic);
    let file = dir.join("plant.ped");
    assert!(app.ped_save_as(&file));
    let page = app.ped_prepare_print().unwrap();
    let html = std::fs::read_to_string(&page).unwrap();
    assert!(html.contains("window.print()"));
    assert!(html.contains("ID: PUMP"), "{html}");
    assert!(html.contains("plant.ped"));

    let mut h = Harness::builder()
        .with_size(vec2(1280.0, 800.0))
        .build_eframe(move |_| app);
    h.run();
    // the window's own menus come after the main window's: take the last
    h.get_all_by_label("File").last().unwrap().click();
    h.run();
    for item in ["Save As...", "Export Text File...", "Print...", "Close"] {
        assert!(h.get_all_by_label(item).count() >= 1, "File > {item}");
    }
    h.get_by_label("Close").click();
    h.run();
    assert!(!h.state().ped.open);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn database_window_edit_menu_acts_on_events() {
    let mut app = App::default();
    app.ped_new();
    app.ped.open = true;
    fill(&mut app, "PUMP", "1e-3", EventType::Basic);
    app.ped_select("PUMP");
    let mut h = Harness::builder()
        .with_size(vec2(1280.0, 800.0))
        .build_eframe(move |_| app);
    h.run();
    h.get_all_by_label("Edit").last().unwrap().click();
    h.run();
    for item in [
        "Apply event",
        "Clear",
        "Delete event",
        "Copy to tree database",
    ] {
        assert!(h.get_all_by_label(item).count() >= 1, "Edit > {item}");
    }
    h.get_all_by_label("Delete event").next().unwrap().click();
    h.run();
    assert!(h.state().ped.event_ids().is_empty());
}

#[test]
fn database_window_is_a_viewport_that_keeps_its_state() {
    // Headless there are no OS windows, so egui embeds it in a window; the
    // contents, state and closing behave the same as in a real window.
    let mut app = App::default();
    app.ped_new();
    app.ped.open = true;
    fill(&mut app, "PUMP", "1e-3", EventType::Basic);
    let mut h = Harness::builder()
        .with_size(vec2(1280.0, 800.0))
        .build_eframe(move |_| app);
    h.run();
    h.get_by_label("PUMP");
    h.state_mut().ped.open = false;
    h.run();
    assert!(h.query_by_label("Primary Events").is_none());
    h.state_mut().ped.open = true;
    h.run();
    h.get_by_label("PUMP"); // reopened with the same events
}

#[test]
fn the_selected_event_is_highlighted_in_the_list() {
    let mut app = App::default();
    app.ped_new();
    app.ped.open = true;
    fill(&mut app, "PUMP", "1e-3", EventType::Basic);
    fill(&mut app, "VALVE", "2e-3", EventType::Basic);
    app.ped_select("VALVE");
    let mut h = Harness::builder()
        .with_size(vec2(1280.0, 800.0))
        .build_eframe(move |_| app);
    h.run();
    let selected = |h: &Harness<'static, App>, id: &str| {
        h.get_all_by_label(id)
            .any(|n| n.accesskit_node().toggled() == Some(eframe::egui::accesskit::Toggled::True))
    };
    assert!(selected(&h, "VALVE"));
    assert!(!selected(&h, "PUMP"));
}

#[test]
fn new_and_open_keep_the_window_open_and_say_what_happened() {
    let mut app = App::default();
    app.ped.open = true;
    app.ped_new();
    assert!(app.ped.open, "New keeps the window open");
    assert_eq!(app.ped.status.as_deref(), Some("New database"));

    let path = data("theatre.ped");
    assert!(app.ped_open(&path));
    assert!(app.ped.open, "Open keeps the window open");
    let n = app.ped.db.events.len();
    assert!(n > 0);
    assert_eq!(
        app.ped.status.as_deref(),
        Some(format!("Opened {} ({n} events)", path.display()).as_str())
    );

    app.ped.open = false;
    app.ped_new();
    assert!(!app.ped.open, "New does not open a closed window");
}
