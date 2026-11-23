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

/// A fixture file, copied (with its siblings) into a fresh temp directory so
/// analyses that write report files beside the tree never touch the repo.
fn data(name: &str) -> PathBuf {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static N: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "ftar_gui_data_{}_{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    for e in std::fs::read_dir(DATA).unwrap().flatten() {
        if e.path().is_file() {
            let _ = std::fs::copy(e.path(), dir.join(e.file_name()));
        }
    }
    dir.join(name)
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
    h.get_all_by_label("Delete").last().unwrap().click(); // the edit panel button, after the toolbar one
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
    h.get_by_label_contains("Quit").click(); // the label carries its shortcut
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
    h.query_all_by_label_contains("New").last().unwrap().click(); // the menu item, after the toolbar button
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
    h.get_all_by_label("New").last().unwrap().click();
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
        let all: Vec<_> = h.query_all_by_label_contains(item).collect(); // labels carry their shortcut
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

// ------------------------------------------------- image export and printing

#[test]
fn export_png_and_svg_from_the_edited_tree() {
    let dir = scratch("images");
    let mut app = App::default();
    app.open(data("theatre.fta"));
    app.image_scale = 1.0;
    let svg = dir.join("t.svg");
    let png = dir.join("t.png");
    app.export_image(&svg).unwrap();
    app.export_image(&png).unwrap();
    assert!(std::fs::read_to_string(&svg)
        .unwrap()
        .contains("MAINS_FAIL"));
    let bytes = std::fs::read(&png).unwrap();
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
    // the export follows edits: a renamed event shows up in the new drawing
    app.select(Some(vec![0, 1]));
    app.set_id("RENAMED_EVENT");
    app.export_image(&svg).unwrap();
    assert!(std::fs::read_to_string(&svg)
        .unwrap()
        .contains("RENAMED_EVENT"));
    // a larger scale gives a larger file
    app.image_scale = 3.0;
    app.export_image(&png).unwrap();
    assert!(std::fs::read(&png).unwrap().len() > bytes.len());
    assert_eq!(app.default_image_name(), "theatre.png");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn image_export_reports_errors() {
    let mut app = App::default();
    assert!(app.export_image(std::path::Path::new("x.png")).is_err());
    app.open(data("theatre.fta"));
    assert!(app.export_image(std::path::Path::new("x.bmp")).is_err());
    assert!(app.error.as_deref().unwrap().contains(".png or .svg"));
    app.image_scale = 0.0;
    assert!(app
        .export_image(&std::env::temp_dir().join("ftar_zero_scale.png"))
        .is_err());
    app.image_scale = 1.0;
    let bad = std::env::temp_dir()
        .join("ftar_no_such_dir_abc")
        .join("x.png");
    assert!(app.export_image(&bad).is_err());
}

#[test]
fn print_page_has_tree_note_and_the_selected_report() {
    let mut app = App::default();
    app.open(data("theatre.fta"));
    app.report_kind = ReportKind::CutSets;
    let html = app.print_page(true).unwrap();
    assert!(html.contains("<svg ") && html.contains("window.print()"));
    assert!(html.contains("Failure Of An Electrical Supply"));
    assert!(html.contains("Minimal Cut Sets"), "report appended");
    assert!(html.contains("<title>theatre</title>"));
    app.print_report = false;
    assert!(!app.print_page(false).unwrap().contains("Minimal Cut Sets"));
    app.image_probabilities = false;
    assert!(!app.print_page(false).unwrap().contains("p = "));
}

#[test]
fn printing_a_tree_whose_analysis_fails_still_prints_the_tree() {
    let mut app = App::default();
    app.open(data("theatre.fta"));
    app.report_kind = ReportKind::Probability;
    // events without database entries make the probability analysis fail
    app.doc.as_mut().unwrap().db.events.clear();
    assert!(app.report_text(ReportKind::Probability, "").is_err());
    let html = app.print_page(false).unwrap();
    assert!(html.contains("<svg ") && html.contains("RELAY_FAIL"));
    assert!(
        !html.contains("class=\"report\""),
        "no report when the analysis fails"
    );
}

#[test]
fn prepare_print_writes_a_page_that_opens_the_print_dialog() {
    let mut app = App::default();
    assert!(app.prepare_print().is_err());
    app.open(data("lift.fta"));
    let path = app.prepare_print().unwrap();
    let html = std::fs::read_to_string(&path).unwrap();
    assert!(html.contains("window.print()") && html.contains("DPD_1"));
    assert!(path.extension().unwrap() == "html");
    let _ = std::fs::remove_file(path);
}

#[test]
fn file_menu_lists_image_export_and_print() {
    let mut h = harness();
    h.run();
    h.get_by_label("File").click();
    h.run();
    for item in [
        "Export image...",
        "Print...",
        "Probabilities in images and prints",
    ] {
        h.get_all_by_label(item).last().unwrap();
    }
    let mut empty = Harness::builder()
        .with_size(vec2(1280.0, 800.0))
        .build_eframe(|_| App::default());
    empty.run();
    empty.get_by_label("File").click();
    empty.run();
    for item in ["Export image...", "Print..."] {
        assert!(
            empty
                .get_all_by_label(item)
                .last()
                .unwrap()
                .accesskit_node()
                .is_disabled(),
            "{item}"
        );
    }
}

// ------------------------------------------------ primary event editor

#[test]
fn event_editor_applies_clears_and_deletes_like_openped() {
    let mut app = App::default();
    app.open(data("lift.fta"));
    app.event_form_load("DOD_1");
    assert_eq!(app.event_form.id, "DOD_1");
    assert_eq!(app.event_form.desc, "Door obsticle detector fails - Off");

    app.event_form.prob = "2.5E-6".into();
    app.event_form.dormant = true;
    app.event_form.desc = "Door obstacle detector fails".into();
    assert!(app.event_form_apply());
    let e = &app.doc.as_ref().unwrap().db.events["DOD_1"];
    assert_eq!((e.probability, e.modifier), (2.5e-6, Some('D')));
    assert!(app.dirty);

    // bad input is refused with a message and leaves the database alone
    app.event_form.prob = "lots".into();
    assert!(!app.event_form_apply());
    assert!(app.error.is_some());
    app.event_form.prob = "0.1".into();
    app.event_form.id = "a;b".into();
    assert!(!app.event_form_apply());

    // a new id adds an event
    app.event_form_clear();
    app.event_form.id = "PRESSURE_1".into();
    app.event_form.prob = "1E-4".into();
    assert!(app.event_form_apply());
    assert!(app.event_ids().contains(&"PRESSURE_1".to_string()));
    assert!(app.event_form_delete());
    assert!(!app.event_ids().contains(&"PRESSURE_1".to_string()));
    app.undo();
    assert!(app.event_ids().contains(&"PRESSURE_1".to_string()));
}

#[test]
fn event_editor_window_has_the_openped_controls() {
    let mut app = App::default();
    app.open(data("lift.fta"));
    app.events_open = true;
    let mut h = harness_for(app);
    h.run();
    for label in [
        "Id",
        "Probability",
        "Description",
        "Basic",
        "External",
        "Undeveloped",
        "Conditional (not analysed)",
        "Conditional (analysed)",
        "Dormant",
        "Constant Failure Rate/Unit Time",
        "Apply",
        "Clear",
        "DOD_1",
    ] {
        // some names also exist elsewhere in the window (analysis tabs)
        assert!(h.get_all_by_label(label).count() >= 1, "{label}");
    }
    h.get_by_label("DOD_1").click();
    h.run();
    assert_eq!(h.state().event_form.id, "DOD_1");
}

// ------------------------------------------------- primary event database

#[test]
fn events_show_which_database_they_come_from_and_it_can_be_changed() {
    let mut app = App::default();
    app.open(data("lift.fta"));
    assert_eq!(app.event_origin("DOD_1"), "Data from database lift.ped");
    assert!(app
        .event_origin("NO_SUCH")
        .contains("Not in database lift.ped"));

    app.set_database("other.ped");
    assert_eq!(
        app.doc.as_ref().unwrap().tree.db_name.as_deref(),
        Some("other.ped")
    );
    assert!(app.dirty);
    // a name that does not end in a file name is refused and changes nothing
    app.set_database("library/");
    assert!(app.error.is_some());
    assert_eq!(
        app.doc.as_ref().unwrap().tree.db_name.as_deref(),
        Some("other.ped")
    );
    app.undo();
    assert_eq!(
        app.doc.as_ref().unwrap().tree.db_name.as_deref(),
        Some("lift.ped")
    );

    // the toolbar has a labelled field for it
    let mut h = harness_for(app);
    h.run();
    h.get_by_label("database");
}

/// A shared database in another folder: attaching reads its events, saving
/// writes back to it rather than to a copy next to the tree.
#[test]
fn a_tree_can_use_a_shared_database_in_another_folder() {
    let base = std::env::temp_dir().join(format!("ftar_gui_shared_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(base.join("a")).unwrap();
    std::fs::create_dir_all(base.join("library")).unwrap();
    std::fs::copy(data("lift.ped"), base.join("library/shared.ped")).unwrap();
    std::fs::copy(data("lift.fta"), base.join("a/lift.fta")).unwrap();

    let mut app = App::default();
    app.open(base.join("a/lift.fta"));
    assert!(app.doc.as_ref().unwrap().db.events.is_empty());
    app.set_database("../library/shared.ped");
    assert!(app.error.is_none(), "{:?}", app.error);
    assert!(app.doc.as_ref().unwrap().db.events.contains_key("DOD_1"));
    assert_eq!(
        app.event_origin("DOD_1"),
        "Data from database ../library/shared.ped"
    );
    app.save_as(base.join("a/lift.fta")).unwrap();
    assert!(!base.join("a/shared.ped").exists());
    assert!(!base.join("a/lift.ped").exists());
    let shared = std::fs::read_to_string(base.join("library/shared.ped")).unwrap();
    assert!(shared.contains("DOD_1;"));
    let _ = std::fs::remove_dir_all(base);
}

/// The probability tab lists primary events alphabetically, as OpenFTA does
/// (theatre.fta has RELAY_FAIL first in the tree).
#[test]
fn probability_tab_lists_events_alphabetically() {
    let mut h = harness();
    h.run();
    h.get_by_label("Probability").click();
    h.run();
    h.get_by_label("Run").click();
    h.run();
    let ids: Vec<_> = h
        .state()
        .prob
        .as_ref()
        .expect("report")
        .events
        .iter()
        .map(|e| e.id.clone())
        .collect();
    assert_eq!(ids, ["GEN_FAIL", "MAINS_FAIL", "RELAY_FAIL"]);
}

/// The event editor says which other trees in the folder use an event from
/// the same database, and sees a new tree once it is saved.
#[test]
fn event_editor_lists_other_trees_that_use_the_event() {
    let dir = scratch("usage");
    for f in ["lift.fta", "lift.ped"] {
        std::fs::copy(data(f), dir.join(f)).unwrap();
    }
    let mut app = App::default();
    app.open(dir.join("lift.fta"));
    assert_eq!(
        app.other_trees_using("DOD_1"),
        "Not used by other trees in this folder"
    );

    std::fs::copy(data("lift.fta"), dir.join("lift_b.fta")).unwrap();
    // answers are cached until the tree is saved (or another one opened)
    assert!(app.other_trees_using("DOD_1").starts_with("Not used"));
    app.save().unwrap();
    assert_eq!(
        app.other_trees_using("DOD_1"),
        "Also used by lift_b.fta (2 times)"
    );
    assert!(app.other_trees_using("NO_SUCH").starts_with("Not used"));

    // shown in the editor for the loaded event
    app.events_open = true;
    app.event_form_load("DOD_1");
    let mut h = harness_for(app);
    h.run();
    h.get_by_label("Also used by lift_b.fta (2 times)");

    let mut fresh = App::default();
    fresh.new_doc();
    assert!(fresh.other_trees_using("X").starts_with("Save the tree"));
    let _ = std::fs::remove_dir_all(dir);
}

// ------------------------------------------- canvas drawn like OpenFTA (#52)

#[test]
fn canvas_draws_openfta_labels_and_clicking_a_symbol_selects_it() {
    use eframe::egui::{Event, Modifiers, PointerButton, Pos2};
    let mut h = harness();
    h.run();
    let scene = h.state().scene().expect("scene");
    // gate ids, probabilities and database descriptions, as OpenFTA shows them
    let texts: Vec<String> = scene
        .prims
        .iter()
        .filter_map(|p| match p {
            ftar_render::scene::Prim::Text { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect();
    for want in ["AND1", "Or1", "0.03", "Mains Power", "Tree: theatre"] {
        assert!(texts.iter().any(|t| t == want), "{want} in {texts:?}");
    }
    // click the middle of MAINS_FAIL's circle on screen
    let it = scene.items.iter().find(|i| i.path == [0, 1]).unwrap();
    let b = it.bbox;
    let (z, origin) = (h.state().zoom, h.state().canvas.min + h.state().pan);
    let pos = Pos2::new(
        origin.x + (b[0] + b[2]) / 2.0 * z,
        origin.y + (b[1] + b[3]) / 2.0 * z,
    );
    h.event(Event::PointerMoved(pos));
    for pressed in [true, false] {
        h.event(Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
        h.run();
    }
    assert_eq!(h.state().selected.as_deref(), Some(&[0usize, 1][..]));
    // clicking empty paper clears the selection
    let empty = h.state().canvas.min + vec2(2.0, 2.0);
    h.event(Event::PointerMoved(empty));
    for pressed in [true, false] {
        h.event(Event::PointerButton {
            pos: empty,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
        h.run();
    }
    assert!(h.state().selected.is_none());
}

// ------------------------------------------- zoom and tree overview (#54)

#[test]
fn zoom_in_out_and_set_zoom_keep_the_centre_and_stop_auto_fit() {
    let mut h = tree_with("lift.fta");
    h.run();
    let centre_world = |a: &App| a.visible_world().center();
    let before = centre_world(h.state());
    let z0 = h.state().zoom;
    h.state_mut().zoom_in();
    let a = h.state();
    assert!((a.zoom - z0 * App::ZOOM_STEP).abs() < 1e-4);
    assert!(!a.auto_fit, "a manual zoom is kept");
    assert!(
        (centre_world(a) - before).length() < 0.5,
        "centre stays put"
    );
    h.state_mut().zoom_out();
    assert!((h.state().zoom - z0).abs() < 1e-4);
    h.state_mut().set_zoom(1.5);
    assert_eq!(h.state().zoom_percent(), 150);
    assert!((centre_world(h.state()) - before).length() < 0.5);
    // limits and nonsense
    h.state_mut().set_zoom(100.0);
    assert_eq!(h.state().zoom, App::ZOOM_RANGE.1);
    h.state_mut().set_zoom(0.0);
    assert_eq!(h.state().zoom, App::ZOOM_RANGE.0);
    h.state_mut().set_zoom(f32::NAN);
    assert_eq!(h.state().zoom, App::ZOOM_RANGE.0);
    h.run();
    assert_eq!(h.state().zoom, App::ZOOM_RANGE.0, "not refitted");
}

#[test]
fn keyboard_and_view_menu_zoom() {
    use eframe::egui::{Key, Modifiers};
    let mut h = tree_with("theatre.fta");
    h.run();
    let z0 = h.state().zoom;
    h.key_press_modifiers(Modifiers::COMMAND, Key::Plus);
    h.run();
    assert!(h.state().zoom > z0, "Ctrl++ zooms in");
    h.key_press_modifiers(Modifiers::COMMAND, Key::Minus);
    h.run();
    assert!((h.state().zoom - z0).abs() < 1e-4, "Ctrl+- zooms out");
    h.get_by_label("View").click();
    h.run();
    h.get_by_label("Set Zoom...").click();
    h.run();
    assert!(h.state().set_zoom_open);
    h.get_by_label("25%").click();
    h.run();
    assert_eq!(h.state().zoom_percent(), 25);
    // the preset and the zoom shown next to Fit
    assert_eq!(h.query_all_by_label("25%").count(), 2);
    h.get_by_label("Fit").click();
    h.run();
    assert!(h.state().auto_fit);
    assert_fully_visible(h.state(), "after Fit");
}

#[test]
fn overview_shows_the_whole_tree_and_clicking_moves_the_view() {
    let mut h = tree_with("lift.fta");
    h.run();
    h.state_mut().set_zoom(2.0);
    h.state_mut().centre_on(eframe::egui::vec2(0.0, 0.0));
    h.get_by_label("View").click();
    h.run();
    h.get_by_label("Tree Overview").click();
    h.run();
    assert!(h.state().overview_open);
    h.get_by_label("Tree Overview"); // the window title
                                     // click near the right edge of the overview: the view moves to the right side of the tree
    use eframe::egui::{Event, Modifiers, PointerButton, Pos2};
    let before = h.state().visible_world().center().x;
    let win = h
        .query_all_by_label("Tree Overview")
        .map(|n| n.rect())
        .reduce(|a, b| if a.area() > b.area() { a } else { b })
        .unwrap();
    let pos = Pos2::new(win.right() - 20.0, win.center().y + 10.0);
    h.event(Event::PointerMoved(pos));
    for pressed in [true, false] {
        h.event(Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
        h.run();
    }
    let after = h.state().visible_world().center().x;
    let scene = h.state().scene().unwrap();
    assert!(
        after > scene.width * 0.6,
        "{before} -> {after} of {}",
        scene.width
    );
    assert!(!h.state().auto_fit);
}

// ------------------------------------------------- cut, copy and paste (#53)

#[test]
fn copy_and_paste_a_branch_under_another_gate_and_undo() {
    let mut app = App::default();
    app.open(data("theatre.fta"));
    // Top -> AND1 -> [Backup_Fail -> Or1 -> [RELAY_FAIL, GEN_FAIL], MAINS_FAIL]
    assert!(!app.can_copy() && !app.can_paste());
    app.select(Some(vec![0, 1]));
    app.copy_selected();
    assert!(app.can_paste());
    assert!(!app.dirty, "copying does not change the tree");
    app.select(Some(vec![0, 0, 0]));
    app.paste();
    assert_eq!(app.selected.as_deref(), Some(&[0usize, 0, 0, 2][..]));
    assert_eq!(children(&app, &[0, 0, 0]), 3);
    assert!(app.dirty && app.error.is_none());
    // pasting again with an event selected puts the copy after it
    app.paste();
    assert_eq!(app.selected.as_deref(), Some(&[0usize, 0, 0, 3][..]));
    app.undo();
    app.undo();
    assert_eq!(children(&app, &[0, 0, 0]), 2);
}

#[test]
fn cut_and_paste_moves_a_branch_and_bad_targets_say_why() {
    let mut app = App::default();
    app.open(data("theatre.fta"));
    app.select(Some(vec![]));
    assert!(
        app.can_copy() && !app.can_cut(),
        "the top event cannot be cut"
    );
    app.select(Some(vec![0, 0]));
    app.cut_selected();
    assert_eq!(children(&app, &[0]), 1, "Backup_Fail is gone");
    assert!(app.status.as_deref().unwrap().contains("Backup_Fail"));
    // the top event already has its gate: refused, tree unchanged
    app.select(Some(vec![]));
    app.paste();
    assert!(app.error.is_some());
    assert_eq!(children(&app, &[0]), 1);
    app.select(Some(vec![0]));
    app.paste();
    assert!(app.error.is_none());
    assert_eq!(children(&app, &[0]), 2);
    assert_eq!(app.selected.as_deref(), Some(&[0usize, 1][..]));
    assert_eq!(app.selected_node().unwrap().id, "Backup_Fail");
    assert_eq!(children(&app, &[0, 1, 0]), 2, "with its whole branch");
}

#[test]
fn pasting_into_another_tree_brings_the_event_data() {
    let mut app = App::default();
    app.open(data("theatre.fta"));
    app.select(Some(vec![0, 1])); // MAINS_FAIL, p = 0.03
    app.copy_selected();
    app.open(data("lift.fta"));
    assert!(
        app.can_paste(),
        "the clipboard survives opening another tree"
    );
    assert!(!app
        .doc
        .as_ref()
        .unwrap()
        .db
        .events
        .contains_key("MAINS_FAIL"));
    app.select(Some(vec![0]));
    app.paste();
    assert!(app.error.is_none(), "{:?}", app.error);
    let e = &app.doc.as_ref().unwrap().db.events["MAINS_FAIL"];
    assert!((e.probability - 0.03).abs() < 1e-12);
}

#[test]
fn edit_menu_and_shortcuts_cut_copy_paste() {
    use eframe::egui::{Key, Modifiers};
    let mut h = harness();
    h.run();
    h.state_mut().select(Some(vec![0, 1]));
    h.run();
    h.key_press_modifiers(Modifiers::COMMAND, Key::C);
    h.run();
    assert!(h.state().clipboard.is_some(), "Ctrl+C copies");
    h.state_mut().select(Some(vec![0, 0, 0]));
    h.key_press_modifiers(Modifiers::COMMAND, Key::V);
    h.run();
    assert_eq!(children(h.state(), &[0, 0, 0]), 3, "Ctrl+V pastes");
    h.key_press_modifiers(Modifiers::COMMAND, Key::X);
    h.run();
    assert_eq!(
        children(h.state(), &[0, 0, 0]),
        2,
        "Ctrl+X cuts the pasted copy"
    );
    // the Edit menu has the three commands, with their shortcuts
    h.get_by_label("Edit").click();
    h.run();
    for item in ["Cut", "Copy", "Paste"] {
        let mut found = h.query_all_by_label_contains(item).filter(|n| {
            n.accesskit_node()
                .label()
                .is_some_and(|l| l.starts_with(item) && l.contains("Ctrl"))
        });
        assert!(found.next().is_some(), "{item} in the Edit menu");
    }
}

#[test]
fn view_theme_menu_switches_and_saves_the_theme() {
    use ftar_render::Theme;
    let dir = std::env::temp_dir().join(format!("ftar_gui_theme_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let file = dir.join("prefs.txt");
    let mut app = App::default();
    app.use_prefs_file(file.clone());
    assert_eq!(app.prefs.theme, Theme::Light, "light like OpenFTA");
    let mut h = Harness::builder()
        .with_size(vec2(1280.0, 800.0))
        .build_eframe(move |_| app);
    h.run();
    assert!(!h.ctx.global_style().visuals.dark_mode);
    h.get_by_label("View").click();
    h.run();
    h.get_by_label("Theme").click();
    h.run();
    h.get_by_label("Dark").click();
    h.run();
    h.run();
    assert_eq!(h.state().prefs.theme, Theme::Dark);
    assert!(h.ctx.global_style().visuals.dark_mode);
    assert_eq!(ftar_render::Preferences::load(&file).theme, Theme::Dark);
    let _ = std::fs::remove_dir_all(&dir);
}

// ------------------------------------------------ #101 OpenFTA's shortcuts

#[test]
fn openftas_shortcuts_act_on_the_tree() {
    use eframe::egui::{Key, Modifiers};
    let mut app = App::default();
    app.open(PathBuf::from(DATA).join("lift.fta"));
    let before = app.doc.as_ref().unwrap().db.events.len();
    let leaf = ftar_core::edit::all_paths(&app.doc.as_ref().unwrap().tree)
        .into_iter()
        .rfind(|p| !p.is_empty())
        .unwrap();
    app.select(Some(leaf));
    let nodes = |a: &App| ftar_core::edit::all_paths(&a.doc.as_ref().unwrap().tree).len();
    let n0 = nodes(&app);
    let mut h = Harness::builder()
        .with_size(vec2(1280.0, 800.0))
        .build_eframe(move |_| app);
    h.run();
    // Ctrl+D deletes the selected node (like OpenFTA, not only the Delete key)
    h.key_press_modifiers(Modifiers::COMMAND, Key::D);
    h.run();
    assert_eq!(nodes(h.state()), n0 - 1, "Ctrl+D deletes");
    h.state_mut().undo();
    // Ctrl+L / Ctrl+R move the node; the selection follows it
    let sel0 = h.state().selected.clone();
    h.key_press_modifiers(Modifiers::COMMAND, Key::L);
    h.run();
    h.key_press_modifiers(Modifiers::COMMAND, Key::R);
    h.run();
    assert_eq!(h.state().selected, sel0, "left then right is where it was");
    // Ctrl+Q asks to quit; a clean document quits at once
    h.key_press_modifiers(Modifiers::COMMAND, Key::Q);
    h.run();
    assert!(h.state().quit || h.state().pending.is_some());
    let _ = before;
}
