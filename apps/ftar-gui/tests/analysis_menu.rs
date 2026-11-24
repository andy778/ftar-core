//! Analysis menu: Validate and the analysis entries (#66).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use eframe::egui::vec2;
use egui_kittest::{
    kittest::{NodeT, Queryable},
    Harness,
};
use ftar_gui::{App, Tab};

const DATA: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../crates/core/tests/data");

fn harness_for(app: App) -> Harness<'static, App> {
    Harness::builder()
        .with_size(vec2(1280.0, 800.0))
        .build_eframe(move |_| app)
}

fn theatre() -> App {
    let mut app = App::default();
    app.open(PathBuf::from(DATA).join("theatre.fta"));
    app
}

fn broken() -> App {
    let mut app = theatre();
    // Top -> AND1 -> [Backup_Fail, MAINS_FAIL]: removing one leaves a one-input gate
    app.select(Some(vec![0, 1]));
    app.delete_selected();
    app
}

#[test]
fn validate_reports_a_clean_tree() {
    let mut h = harness_for(theatre());
    h.run();
    h.get_by_label("Analysis").click();
    h.run();
    h.get_by_label("Validate").click();
    h.run();
    assert!(h.state().validate_open);
    assert!(h
        .state()
        .status
        .as_deref()
        .unwrap()
        .contains("no structural problems"));
    h.get_by_label("No structural problems found.");
}

#[test]
fn validate_lists_problems_and_clicking_one_selects_its_node() {
    let mut h = harness_for(broken());
    h.state_mut().select(None);
    h.run();
    assert_eq!(h.state().issues().len(), 1);
    h.get_by_label("Analysis").click();
    h.run();
    h.get_by_label("Validate").click();
    h.run();
    assert!(h.state().status.as_deref().unwrap().contains("1 problem"));
    h.get_by_label_contains("AND1: gate needs at least two children")
        .click();
    h.run();
    assert_eq!(h.state().selected.as_deref(), Some(&[0usize][..]));
}

#[test]
fn validate_lists_the_same_structural_issues_as_the_core_check() {
    // (the headless tool adds a warning for transfers opened raw; the GUI keeps them as symbols)
    let app = broken();
    let ours: Vec<Vec<usize>> = app.issues().iter().map(|i| i.path.clone()).collect();
    let doc = app.doc.as_ref().unwrap();
    let theirs: Vec<Vec<usize>> = ftar_core::edit::validate(&doc.tree)
        .iter()
        .map(|i| i.path.clone())
        .collect();
    assert_eq!(ours, theirs);
}

#[test]
fn analysis_menu_shows_each_analysis_and_is_disabled_without_a_tree() {
    let mut h = harness_for(theatre());
    h.run();
    for (item, tab) in [
        ("Numerical Probability...", Tab::Probability),
        ("Monte Carlo Simulation...", Tab::MonteCarlo),
        ("Minimal Cut Sets...", Tab::CutSets),
    ] {
        h.get_by_label("Analysis").click();
        h.run();
        h.get_by_label(item).click();
        h.run();
        assert_eq!(h.state().tab, tab, "{item}");
        assert_eq!(h.state().report_kind, App::kind_for_tab(tab));
    }
    let mut empty = harness_for(App::default());
    empty.run();
    empty.get_by_label("Analysis").click();
    empty.run();
    assert!(empty
        .get_by_label("Validate")
        .accesskit_node()
        .is_disabled());
}
