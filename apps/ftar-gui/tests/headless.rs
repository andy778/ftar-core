//! Headless GUI tests: no window, no GPU. Drives the real egui app through
//! kittest, and the operations behind it directly.

use std::path::PathBuf;

use eframe::egui::vec2;
use egui_kittest::{
    kittest::{NodeT, Queryable},
    Harness,
};
use ftar_core::analysis::{probability_report, ProbabilityOptions};
use ftar_core::EventType;
use ftar_gui::{App, Pending, Tab};

const DATA: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../crates/core/tests/data");

fn data(name: &str) -> PathBuf {
    PathBuf::from(DATA).join(name)
}

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("ftar_gui_{}_{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn harness_for(app: App) -> Harness<'static, App> {
    Harness::builder()
        .with_size(vec2(1280.0, 800.0))
        .build_eframe(move |_| app)
}

fn harness() -> Harness<'static, App> {
    let mut app = App::default();
    app.open(data("theatre.fta"));
    harness_for(app)
}

fn exact_top(app: &App) -> f64 {
    let p = app.doc.as_ref().unwrap().analysis_project().unwrap();
    probability_report(&p.tree, &p.db, &ProbabilityOptions::default())
        .unwrap()
        .top
}

fn children(app: &App, path: &[usize]) -> usize {
    ftar_core::edit::node_at(&app.doc.as_ref().unwrap().tree, path)
        .unwrap()
        .children
        .len()
}

// ------------------------------------------------------------ viewing

#[test]
fn shows_hint_without_a_file() {
    let mut h = Harness::builder().build_eframe(|_| App::default());
    h.run();
    h.get_by_label_contains("Open a .fta file");
}

#[test]
fn loads_tree_and_runs_cut_set_analysis() {
    let mut h = harness();
    h.run();
    assert!(h.state().doc.is_some(), "{:?}", h.state().error);
    h.get_by_label("Run").click();
    h.run();
    assert_eq!(h.state().cut_sets.as_ref().map(Vec::len), Some(2));
    h.get_by_label_contains("2 minimal cut sets");
}

#[test]
fn probability_tab_shows_exact_value() {
    let mut h = harness();
    h.run();
    h.get_by_label("Probability").click();
    h.run();
    h.get_by_label("Run").click();
    h.run();
    assert_eq!(h.state().tab, Tab::Probability);
    let r = h.state().prob.as_ref().expect("report");
    assert!(r.exact && (r.top - 2.0643378107574753e-3).abs() < 1e-12);
    h.get_by_label_contains("Exact value");
}

#[test]
fn bad_file_reports_error_instead_of_panicking() {
    let mut app = App::default();
    app.open("does-not-exist.fta".into());
    let mut h = harness_for(app);
    h.run();
    assert!(h.state().error.is_some());
    assert!(h.state().doc.is_none());
}

// ------------------------------------------------------------ editing

#[test]
fn build_a_tree_save_reopen_and_analyse() {
    let dir = scratch("build");
    let mut app = App::default();
    app.new_doc();
    app.select(Some(vec![0])); // the OR gate
    app.add_child(EventType::Basic);
    let a = app.selected_node().unwrap().id.clone();
    app.select(Some(vec![0]));
    app.add_child(EventType::Basic);
    let b = app.selected_node().unwrap().id.clone();
    assert_ne!(a, b);
    app.set_event(&a, 0.1, "first", false);
    app.set_event(&b, 0.2, "second", false);
    assert!(app.dirty);
    assert!((exact_top(&app) - 0.28).abs() < 1e-12);

    assert!(app.save().is_err(), "no path yet");
    let path = dir.join("t.fta");
    app.save_as(path.clone()).unwrap();
    assert!(!app.dirty);
    assert!(path.is_file() && dir.join("t.ped").is_file());

    let mut again = App::default();
    again.open(path);
    assert_eq!(again.doc.as_ref().unwrap().db.events.len(), 2);
    assert!((exact_top(&again) - 0.28).abs() < 1e-12);
    assert_eq!(
        again.doc.as_ref().unwrap().db.events[&a].description,
        "first"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn undo_redo_cover_tree_and_database() {
    let mut app = App::default();
    app.open(data("theatre.fta"));
    let before = exact_top(&app);
    app.select(Some(vec![0]));
    let n = children(&app, &[0]);
    app.add_child(EventType::Basic);
    assert_eq!(children(&app, &[0]), n + 1);
    let id = app.selected_node().unwrap().id.clone();
    app.set_event(&id, 0.5, "", false);
    assert!(exact_top(&app) != before);
    app.undo();
    app.undo();
    assert_eq!(children(&app, &[0]), n);
    assert!(!app.doc.as_ref().unwrap().db.events.contains_key(&id));
    assert!((exact_top(&app) - before).abs() < 1e-15);
    app.redo();
    assert_eq!(children(&app, &[0]), n + 1);
}

#[test]
fn rejected_edits_change_nothing_and_say_why() {
    let mut app = App::default();
    app.open(data("theatre.fta"));
    let tree = app.doc.as_ref().unwrap().tree.clone();
    let can_undo = app.history.can_undo();
    // find a basic event and try to hang a child under it
    let leaf = ftar_core::edit::all_paths(&tree)
        .into_iter()
        .find(|p| ftar_core::edit::node_at(&tree, p).unwrap().kind == EventType::Basic)
        .unwrap();
    app.select(Some(leaf));
    app.add_child(EventType::Basic);
    assert!(app.error.is_some());
    assert_eq!(app.doc.as_ref().unwrap().tree, tree);
    assert_eq!(app.history.can_undo(), can_undo);
    assert!(!app.dirty);
    // the root cannot be deleted
    app.select(Some(vec![]));
    app.delete_selected();
    assert!(app.error.is_some());
    assert_eq!(app.doc.as_ref().unwrap().tree, tree);
}

#[test]
fn renaming_an_event_carries_its_database_entry() {
    let mut app = App::default();
    app.open(data("theatre.fta"));
    let tree = app.doc.as_ref().unwrap().tree.clone();
    let leaf = ftar_core::edit::all_paths(&tree)
        .into_iter()
        .find(|p| ftar_core::edit::node_at(&tree, p).unwrap().id == "RELAY_FAIL")
        .unwrap();
    app.select(Some(leaf));
    app.set_id("RELAY_2");
    let db = &app.doc.as_ref().unwrap().db;
    assert!(db.events.contains_key("RELAY_2") && !db.events.contains_key("RELAY_FAIL"));
    assert!((db.events["RELAY_2"].probability - 0.05).abs() < 1e-12);
    app.set_id("bad id"); // whitespace is not allowed
    assert!(app.error.is_some());
}

#[test]
fn type_change_survives_save_and_reload() {
    let dir = scratch("kind");
    let mut app = App::default();
    app.open(data("theatre.fta"));
    let tree = app.doc.as_ref().unwrap().tree.clone();
    let leaf = ftar_core::edit::all_paths(&tree)
        .into_iter()
        .find(|p| ftar_core::edit::node_at(&tree, p).unwrap().id == "MAINS_FAIL")
        .unwrap();
    app.select(Some(leaf.clone()));
    app.set_kind(EventType::Undeveloped);
    app.save_as(dir.join("k.fta")).unwrap();
    let mut again = App::default();
    again.open(dir.join("k.fta"));
    let n = ftar_core::edit::node_at(&again.doc.as_ref().unwrap().tree, &leaf).unwrap();
    assert_eq!(n.kind, EventType::Undeveloped);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn move_reorders_siblings() {
    let mut app = App::default();
    app.open(data("theatre.fta"));
    app.select(Some(vec![0]));
    let first = ftar_core::edit::node_at(&app.doc.as_ref().unwrap().tree, &[0, 0])
        .unwrap()
        .clone();
    app.select(Some(vec![0, 0]));
    app.move_selected(1);
    assert_eq!(app.selected, Some(vec![0, 1]));
    let moved = ftar_core::edit::node_at(&app.doc.as_ref().unwrap().tree, &[0, 1]).unwrap();
    assert_eq!(*moved, first);
}

#[test]
fn transfer_trees_stay_unexpanded_when_saved() {
    let dir = scratch("transfer");
    for f in ["top.fta", "middle.fta", "bottom.fta", "bottom.ped"] {
        std::fs::copy(data(f), dir.join(f)).unwrap();
    }
    let mut app = App::default();
    app.open(dir.join("top.fta"));
    app.run_analysis();
    assert_eq!(
        app.cut_sets.as_ref().map(Vec::len),
        Some(2),
        "{:?}",
        app.error
    );
    app.select(Some(vec![]));
    app.set_text("edited");
    app.save().unwrap();
    let text = std::fs::read_to_string(dir.join("top.fta")).unwrap();
    assert!(text.contains("I middle.fta"), "{text}");
    assert!(text.contains("edited"));
    let _ = std::fs::remove_dir_all(dir);
}

// ------------------------------------------------- unsaved-changes prompt

#[test]
fn unsaved_changes_are_protected() {
    let mut app = App::default();
    app.open(data("theatre.fta"));
    app.select(Some(vec![0]));
    app.add_child(EventType::Basic);
    assert!(app.dirty);

    app.request_open(data("lift.fta"));
    assert!(matches!(app.pending, Some(Pending::Open(_))));
    assert!(app.title().contains("theatre"), "nothing opened yet");
    app.cancel_pending();
    assert!(app.dirty && app.pending.is_none());

    app.request_new();
    assert_eq!(app.pending, Some(Pending::New));
    app.resolve_pending(false);
    assert!(app.title().starts_with("untitled") && !app.dirty);

    app.new_doc();
    app.select(Some(vec![0]));
    app.add_child(EventType::Basic);
    app.request_quit();
    assert_eq!(app.pending, Some(Pending::Quit));
    assert!(!app.quit);
    // "Save" on a document without a file name cannot succeed silently
    app.resolve_pending(true);
    assert!(!app.quit && app.pending.is_some());
    app.resolve_pending(false);
    assert!(app.quit);
}

// ------------------------------------------------------------- UI clicks

#[test]
fn clicking_add_child_and_undo_in_the_ui() {
    let mut app = App::default();
    app.open(data("theatre.fta"));
    app.select(Some(vec![0]));
    let n = children(&app, &[0]);
    let mut h = harness_for(app);
    h.run();
    h.get_by_label("Add child").click();
    h.run();
    assert_eq!(children(h.state(), &[0]), n + 1);
    assert!(h.state().dirty);
    h.get_by_label_contains("theatre.fta *");
    h.get_by_label("Undo").click();
    h.run();
    assert_eq!(children(h.state(), &[0]), n);
    h.get_by_label("Redo").click();
    h.run();
    assert_eq!(children(h.state(), &[0]), n + 1);
}

#[test]
fn clicking_delete_removes_the_selected_node() {
    let mut app = App::default();
    app.open(data("theatre.fta"));
    app.select(Some(vec![0, 0]));
    let n = children(&app, &[0]);
    let mut h = harness_for(app);
    h.run();
    h.get_by_label("Delete").click();
    h.run();
    assert_eq!(children(h.state(), &[0]), n - 1);
    assert_eq!(h.state().selected, Some(vec![0]));
}

#[test]
fn unsaved_prompt_buttons_work() {
    let mut app = App::default();
    app.open(data("theatre.fta"));
    app.select(Some(vec![0]));
    app.add_child(EventType::Basic);
    app.request_new();
    let mut h = harness_for(app);
    h.run();
    h.get_by_label_contains("Save your changes");
    h.get_by_label("Cancel").click();
    h.run();
    assert!(h.state().pending.is_none() && h.state().dirty);
    h.state_mut().request_new();
    h.run();
    h.get_by_label("Discard").click();
    h.run();
    assert!(h.state().title().starts_with("untitled"));
}

// ---------------------------------------------------------------- reports

use ftar_core::report::ReportKind;

#[test]
fn every_report_kind_renders_and_is_reproducible() {
    let mut app = App::default();
    app.open(data("theatre.fta"));
    app.trials = 5_000;
    for kind in ReportKind::ALL {
        let a = app.report_text(kind, "T").unwrap();
        let b = app.report_text(kind, "T").unwrap();
        assert_eq!(a, b, "{kind:?}");
        assert!(a.contains("Tree   : theatre"), "{kind:?}\n{a}");
        assert!(a.contains("Time   : T"), "{kind:?}");
    }
    assert!(app
        .report_text(ReportKind::Probability, "")
        .unwrap()
        .contains("Exact value"));
}

#[test]
fn export_writes_the_report_for_the_edited_tree() {
    let dir = scratch("report");
    let mut app = App::default();
    app.open(data("theatre.fta"));
    app.report_kind = ReportKind::CutSets;
    let before = dir.join("before.txt");
    app.export_report(&before).unwrap();
    // remove one branch: the report must change
    app.select(Some(vec![0, 1]));
    app.delete_selected();
    let after = dir.join("after.txt");
    app.export_report(&after).unwrap();
    let (a, b) = (
        std::fs::read_to_string(before).unwrap(),
        std::fs::read_to_string(after).unwrap(),
    );
    assert!(a.contains("Minimal Cut Sets") && a != b);
    assert!(app.default_report_name().ends_with("theatre_cutsets.txt"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn export_without_a_tree_or_to_a_bad_path_reports_errors() {
    let mut app = App::default();
    assert!(app.export_report(std::path::Path::new("x.txt")).is_err());
    assert!(app.error.is_some());
    app.open(data("theatre.fta"));
    let bad = std::env::temp_dir()
        .join("ftar_no_such_dir_xyz")
        .join("r.txt");
    assert!(app.export_report(&bad).is_err());
    assert!(app.error.as_deref().unwrap().contains("r.txt"));
}

#[test]
fn report_controls_are_present_and_disabled_without_a_tree() {
    let mut h = harness();
    h.run();
    h.get_by_label("Export report...");
    h.get_by_label("Copy report");
    let mut empty = Harness::builder()
        .with_size(vec2(1280.0, 800.0))
        .build_eframe(|_| App::default());
    empty.run();
    assert!(empty
        .get_by_label("Export report...")
        .accesskit_node()
        .is_disabled());
}

// -------------------------------------------------------------------- fit

fn assert_fully_visible(app: &App, what: &str) {
    let canvas = app.canvas;
    let b = app.content_bounds().expect("bounds");
    assert!(
        canvas.contains_rect(b),
        "{what}: drawing {b:?} sticks out of canvas {canvas:?}"
    );
}

fn tree_with(file: &str) -> Harness<'static, App> {
    let mut app = App::default();
    app.open(data(file));
    harness_for(app)
}

#[test]
fn whole_tree_is_visible_after_opening() {
    for f in ["theatre.fta", "lift.fta", "top.fta"] {
        let mut h = tree_with(f);
        h.run();
        assert_fully_visible(h.state(), f);
    }
}

#[test]
fn tree_stays_fitted_when_the_edit_panel_grows() {
    // selecting a primary event opens the (taller) database row under the canvas
    let mut h = tree_with("lift.fta");
    h.run();
    assert_fully_visible(h.state(), "before select");
    let tree = h.state().doc.as_ref().unwrap().tree.clone();
    let leaf = ftar_core::edit::all_paths(&tree)
        .into_iter()
        .find(|p| ftar_core::edit::node_at(&tree, p).unwrap().kind == EventType::Basic)
        .unwrap();
    let before = h.state().canvas.height();
    h.state_mut().select(Some(leaf));
    h.run();
    assert!(h.state().canvas.height() < before, "panel should take room");
    assert_fully_visible(h.state(), "after select");
}

#[test]
fn tree_stays_fitted_when_it_grows_or_the_window_resizes() {
    let mut h = tree_with("theatre.fta");
    h.run();
    h.state_mut().select(Some(vec![0]));
    for _ in 0..6 {
        h.state_mut().add_child(EventType::Basic);
        h.state_mut().select(Some(vec![0]));
    }
    h.run();
    assert_fully_visible(h.state(), "after adding nodes");
    h.set_size(vec2(700.0, 500.0));
    h.run();
    assert_fully_visible(h.state(), "after resize");
}

#[test]
fn user_pan_and_zoom_turn_auto_fit_off_and_fit_button_restores_it() {
    let mut h = tree_with("theatre.fta");
    h.run();
    assert!(h.state().auto_fit);
    h.state_mut().auto_fit = false;
    h.state_mut().zoom = 3.0;
    h.state_mut().pan = vec2(-500.0, -500.0);
    h.run();
    assert!(h.state().zoom == 3.0, "manual view must not be overridden");
    h.get_by_label("Fit").click();
    h.run();
    assert!(h.state().auto_fit);
    assert_fully_visible(h.state(), "after Fit");
}

// ------------------------------------------------- File menu, Monte Carlo tab

use ftar_core::montecarlo::Method;

#[test]
fn file_menu_new_and_quit_with_the_unsaved_prompt() {
    let mut app = App::default();
    app.open(data("theatre.fta"));
    app.select(Some(vec![0]));
    app.add_child(EventType::Basic);
    let mut h = harness_for(app);
    h.run();
    h.get_by_label("File").click();
    h.run();
    h.get_by_label("Quit").click();
    h.run();
    assert_eq!(
        h.state().pending,
        Some(Pending::Quit),
        "dirty tree must ask first"
    );
    assert!(!h.state().quit);
    h.get_by_label("Cancel").click();
    h.run();
    h.get_by_label("File").click();
    h.run();
    h.get_by_label("New").click();
    h.run();
    assert_eq!(h.state().pending, Some(Pending::New));
}

#[test]
fn file_menu_new_starts_an_untitled_tree_when_clean() {
    let mut h = Harness::builder()
        .with_size(vec2(1280.0, 800.0))
        .build_eframe(|_| App::default());
    h.run();
    h.get_by_label("File").click();
    h.run();
    h.get_by_label("New").click();
    h.run();
    assert!(h.state().title().starts_with("untitled"));
}

#[test]
fn file_menu_save_and_exports_are_disabled_when_there_is_nothing_to_save() {
    let mut h = Harness::builder()
        .with_size(vec2(1280.0, 800.0))
        .build_eframe(|_| App::default());
    h.run();
    h.get_by_label("File").click();
    h.run();
    // "Export report..." exists both in the menu and in the analysis panel: all instances
    for item in ["Save", "Save As...", "Export report..."] {
        let all: Vec<_> = h.get_all_by_label(item).collect();
        assert!(!all.is_empty(), "{item} missing");
        for n in all {
            assert!(
                n.accesskit_node().is_disabled(),
                "{item} should be disabled"
            );
        }
    }
    assert!(h
        .get_by_label("Export Monte Carlo failure modes (CSV)...")
        .accesskit_node()
        .is_disabled());
}

#[test]
fn edit_menu_undo_works() {
    let mut app = App::default();
    app.open(data("theatre.fta"));
    app.select(Some(vec![0]));
    let n = children(&app, &[0]);
    app.add_child(EventType::Basic);
    let mut h = harness_for(app);
    h.run();
    h.get_by_label("Edit").click();
    h.run();
    // the menu entry and the toolbar button share the name; the menu one is last
    h.get_all_by_label("Undo").last().unwrap().click();
    h.run();
    assert_eq!(children(h.state(), &[0]), n);
}

#[test]
fn monte_carlo_tab_selects_its_report_and_shows_failure_modes() {
    let mut app = App::default();
    app.open(data("theatre.fta"));
    app.trials = 20_000;
    let mut h = harness_for(app);
    h.run();
    h.get_by_label("Monte Carlo").click();
    h.run();
    assert_eq!(h.state().report_kind, ReportKind::MonteCarlo);
    assert!(h
        .get_by_label("Export failure modes (CSV)...")
        .accesskit_node()
        .is_disabled());
    h.get_by_label("Run").click();
    h.run();
    h.get_by_label_contains("Failure modes (");
    assert!(!h
        .get_by_label("Export failure modes (CSV)...")
        .accesskit_node()
        .is_disabled());
    h.get_by_label("Probability").click();
    h.run();
    assert_eq!(h.state().report_kind, ReportKind::Probability);
}

#[test]
fn failure_modes_csv_is_well_formed_and_consistent() {
    let dir = scratch("csv");
    let mut app = App::default();
    app.open(data("theatre.fta"));
    assert!(app.modes_csv().is_err(), "needs a run first");
    app.trials = 50_000;
    app.tab = Tab::MonteCarlo;
    app.run_analysis();
    let path = dir.join("m.csv");
    app.export_modes_csv(&path).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    let mut lines = text.lines();
    assert_eq!(lines.next(), Some("rank,failures,probability,events"));
    let r = app.mc.as_ref().unwrap();
    let rows: Vec<&str> = lines.collect();
    assert_eq!(rows.len(), r.modes.len());
    let total: u64 = rows
        .iter()
        .map(|l| l.split(',').nth(1).unwrap().parse::<u64>().unwrap())
        .sum();
    assert_eq!(total, r.system_failures);
    let p: f64 = rows
        .iter()
        .map(|l| l.split(',').nth(2).unwrap().parse::<f64>().unwrap())
        .sum();
    assert!((p - r.probability).abs() < 1e-12);
    assert!(app
        .default_modes_csv_name()
        .ends_with("theatre_failure_modes.csv"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn csv_quotes_event_ids_that_contain_commas() {
    let mut app = App::default();
    app.open(data("theatre.fta"));
    let tree = app.doc.as_ref().unwrap().tree.clone();
    let leaf = ftar_core::edit::all_paths(&tree)
        .into_iter()
        .find(|p| ftar_core::edit::node_at(&tree, p).unwrap().id == "RELAY_FAIL")
        .unwrap();
    app.select(Some(leaf));
    app.set_id("RELAY,2");
    app.trials = 200_000;
    app.tab = Tab::MonteCarlo;
    app.run_analysis();
    let csv = app.modes_csv().unwrap();
    assert!(
        csv.lines()
            .skip(1)
            .any(|l| l.ends_with("\"MAINS_FAIL RELAY,2\"")),
        "{csv}"
    );
}

#[test]
fn monte_carlo_seed_and_method_are_honoured() {
    let run = |seed, method| {
        let mut app = App::default();
        app.open(data("theatre.fta"));
        app.trials = 20_000;
        app.seed = seed;
        app.mc_method = method;
        app.tab = Tab::MonteCarlo;
        app.run_analysis();
        app.mc.unwrap()
    };
    let a = run(1, Method::FirstFailure);
    assert_eq!(
        a.system_failures,
        run(1, Method::FirstFailure).system_failures
    );
    assert_ne!(a.modes, run(2, Method::FirstFailure).modes);
    let d = run(1, Method::Direct);
    assert_eq!(d.method, Method::Direct);
    assert_eq!(d.bas_prob, 1.0);
    // the exported text report uses the same settings
    let mut app = App::default();
    app.open(data("theatre.fta"));
    app.trials = 20_000;
    app.seed = 5;
    app.mc_method = Method::Direct;
    let text = app.report_text(ReportKind::MonteCarlo, "").unwrap();
    assert!(
        !text.contains("Only runs with at least one"),
        "direct method has no such note"
    );
}
