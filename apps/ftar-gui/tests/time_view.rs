//! The Time tab: PFTA's time-dependent analysis in the GUI.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use eframe::egui::vec2;
use egui_kittest::{kittest::Queryable, Harness};
use ftar_core::report::ReportKind;
use ftar_gui::{chart_points, App, Curve, Tab};

const DATA: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../crates/core/tests/data");

fn harness_for(app: App) -> Harness<'static, App> {
    Harness::builder()
        .with_size(vec2(1280.0, 900.0))
        .build_eframe(move |_| app)
}

fn open(name: &str) -> App {
    let mut app = App::default();
    app.open(PathBuf::from(DATA).join(name));
    assert!(app.doc.is_some(), "{name} opens");
    app
}

fn pumps() -> App {
    let mut app = open("text/pumps.ftt");
    app.tab = Tab::Time;
    app.time.samples = 20;
    app
}

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("ftar_gui_time_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

#[test]
fn run_uses_the_trees_settings_and_shows_the_top_gate() {
    let mut app = pumps();
    assert_eq!(app.tree_times(), "0, 1000, 2000, 5000");
    app.run_analysis();
    assert!(app.error.is_none(), "{:?}", app.error);
    let r = app.time.report.as_ref().unwrap();
    assert_eq!(r.times, [0.0, 1000.0, 2000.0, 5000.0]);
    assert_eq!(r.sample_size, 20);
    assert_eq!(app.time.shown_gate().unwrap().id, "NO_FLOW");
    assert_eq!(app.time.at, 3, "importance at the last time");
    assert_eq!(app.time.shown_gate().unwrap().importances.len(), 3);
    // The OpenFTA analyses are untouched.
    assert!(app.prob.is_none() && app.mc.is_none() && app.cut_sets.is_none());
}

#[test]
fn settings_override_the_tree_and_bad_times_are_reported() {
    let mut app = pumps();
    app.time.times = "10, 20".into();
    app.time.samples = 1;
    app.time.seed = "x".into();
    app.time.order = 1;
    app.run_time_analysis();
    let r = app.time.report.clone().unwrap();
    assert_eq!(r.times, [10.0, 20.0]);
    assert_eq!((r.sample_size, r.seed.as_str()), (1, "x"));
    for bad in ["a", "1, -2"] {
        app.time.times = bad.into();
        app.run_time_analysis();
        assert!(
            app.error.as_deref().unwrap().contains("not a time"),
            "{bad}"
        );
        assert_eq!(
            app.time.report.as_ref().unwrap().times,
            r.times,
            "the last result stays"
        );
    }
}

#[test]
fn an_openfta_tree_is_quantified_at_time_one() {
    let mut app = open("theatre.fta");
    app.tab = Tab::Time;
    assert_eq!(app.tree_times(), "1");
    app.run_analysis();
    let g = app.time.shown_gate().unwrap();
    assert!((g.series.q[0] - 2.064_337_810_757_475_3e-3).abs() < 1e-15);
}

#[test]
fn tables_are_exported_with_pfta_layout() {
    let mut app = pumps();
    let dir = scratch("export");
    assert!(
        app.export_time_tables(&dir).is_err(),
        "nothing to export yet"
    );
    app.run_time_analysis();
    assert_eq!(app.export_time_tables(&dir).unwrap(), 5);
    for f in [
        "models.tsv",
        "events.tsv",
        "gates.tsv",
        "cut-sets/NO_FLOW.tsv",
        "importances/NO_FLOW.tsv",
    ] {
        assert!(dir.join(f).is_file(), "{f}");
    }
    assert!(app.status.as_deref().unwrap().contains("5 tables"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn chart_points_follow_the_curve() {
    let mut app = pumps();
    app.run_time_analysis();
    let r = app.time.report.clone().unwrap();
    let g = app.time.shown_gate().unwrap();
    let pts = chart_points(&r, &g.series, Curve::Probability);
    assert_eq!(pts.len(), 4);
    assert_eq!(pts[0], (0.0, 0.0, 0.0, 0.0));
    for (t, mean, lo, hi) in &pts[1..] {
        assert!(
            *t > 0.0 && *lo <= *mean && *mean <= *hi,
            "{t}: {lo} {mean} {hi}"
        );
    }
    assert!(pts.windows(2).all(|w| w[0].1 < w[1].1), "q grows with time");
    let rate = chart_points(&r, &g.series, Curve::Rate);
    let w = chart_points(&r, &g.series, Curve::Intensity);
    assert!(rate[3].1 >= w[3].1, "rate = ω / (1 - q) ≥ ω");
    // `nan` times are not drawn.
    let mut app = pumps();
    app.time.times = "nan, 5".into();
    app.run_time_analysis();
    let r = app.time.report.clone().unwrap();
    assert_eq!(
        chart_points(&r, &r.gates[0].series, Curve::Probability).len(),
        1
    );
}

#[test]
fn the_time_tab_in_the_window() {
    let mut h = harness_for(pumps());
    h.state_mut().tab = Tab::CutSets;
    h.run();
    let before = h.state().report_kind;
    h.get_by_label("Analysis").click();
    h.run();
    h.get_by_label("Time Analysis...").click();
    h.run();
    assert_eq!(h.state().tab, Tab::Time);
    assert_eq!(
        h.state().report_kind,
        before,
        "the OpenFTA report choice stays"
    );
    // Its own settings; not the cut-set order limit.
    for l in ["times", "samples", "seed", "truncation order"] {
        assert!(h.query_by_label(l).is_some(), "{l}");
    }
    assert!(h.query_by_label("max order (0 = all)").is_none());
    h.get_by_label("Run").click();
    h.run();
    h.get_by_label_contains("Gate NO_FLOW (means of 20 samples)");
    h.get_by_label_contains("Importance at time");
    assert!(h.query_by_label("Export tables...").is_some());
    // Export with the dialog cancelled (as under test) changes nothing.
    h.get_by_label("Export tables...").click();
    h.run();
    assert!(h.state().error.is_none());
    // The chart can show the intensity instead.
    h.state_mut().time.curve = Curve::Intensity;
    h.run();
    // An error is shown in the panel.
    h.state_mut().time.times = "soon".into();
    h.get_by_label("Run").click();
    h.run();
    // In the panel and in the status bar.
    assert!(
        h.query_all_by_label_contains("`soon` is not a time")
            .count()
            >= 1
    );
}

#[test]
fn switching_tabs_keeps_the_report_kind_of_the_openfta_tabs() {
    let mut h = harness_for(pumps());
    h.state_mut().tab = Tab::Probability;
    h.state_mut().report_kind = ReportKind::Probability;
    h.run();
    h.get_by_label("Time").click();
    h.run();
    assert_eq!(h.state().tab, Tab::Time);
    assert_eq!(h.state().report_kind, ReportKind::Probability);
    h.get_by_label("Monte Carlo").click();
    h.run();
    assert_eq!(h.state().report_kind, ReportKind::MonteCarlo);
}
