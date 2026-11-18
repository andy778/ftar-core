//! Headless GUI tests: no window, no GPU. Drives the real egui app through kittest.

use egui_kittest::{kittest::Queryable, Harness};
use ftar_gui::{App, Tab};

const THEATRE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../tests/data/theatre.fta");

fn harness() -> Harness<'static, App> {
    let mut app = App::default();
    app.open(THEATRE.into());
    Harness::builder().with_size(eframe::egui::vec2(1280.0, 800.0)).build_eframe(move |_| app)
}

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
    assert!(h.state().project.is_some(), "{:?}", h.state().error);
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
    let mut h = Harness::builder().build_eframe(move |_| app);
    h.run();
    assert!(h.state().error.is_some());
}
