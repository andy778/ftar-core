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

// ------------------------------------------------------------- details

use eframe::egui::{pos2, Rect};
use ftar_gui::{chart_shape, fmt_time, gate_title, TimeView};

#[test]
fn settings_map_to_options() {
    let mut v = TimeView::default();
    let o = v.options().unwrap();
    assert_eq!(
        (o.times, o.sample_size, o.seed, o.computational_order),
        (None, None, None, None),
        "empty and 0 leave the tree's own"
    );
    v.samples = 1;
    v.order = 1;
    v.seed = " s ".into();
    let o = v.options().unwrap();
    assert_eq!((o.sample_size, o.computational_order), (Some(1), Some(1)));
    assert_eq!(o.seed.as_deref(), Some("s"));
    v.order = 3;
    assert_eq!(v.options().unwrap().computational_order, Some(3));
}

#[test]
fn names_in_the_lists() {
    let labels: Vec<&str> = Curve::ALL.iter().map(|c| c.label()).collect();
    assert_eq!(labels, ["probability q", "intensity ω", "rate λ"]);
    let mut app = pumps();
    app.run_time_analysis();
    assert_eq!(gate_title(app.time.shown_gate()), "NO_FLOW (top)");
    assert_eq!(gate_title(None), "");
    assert_eq!(fmt_time(Some(5000.0)), "5000");
    assert_eq!(fmt_time(Some(0.5)), "0.5");
    assert_eq!(fmt_time(None), "");
}

#[test]
fn the_shown_gate_stays_across_runs() {
    let dir = scratch("gates");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("two.ftt");
    std::fs::write(
        &path,
        "- times: 1, 2\n\nGate: TOP\n- type: OR\n- inputs: SUB, C\n\n\
Gate: SUB\n- type: AND\n- inputs: A, B\n\n\
Event: A\n- probability: 0.1\n\nEvent: B\n- probability: 0.2\n\nEvent: C\n- probability: 0.3\n",
    )
    .unwrap();
    let mut app = App::default();
    app.open(path);
    app.tab = Tab::Time;
    app.run_time_analysis();
    assert_eq!(
        app.time.shown_gate().unwrap().id,
        "TOP",
        "the top gate first"
    );
    app.time.gate = 1;
    app.time.at = 0;
    app.run_time_analysis();
    assert_eq!(
        app.time.shown_gate().unwrap().id,
        "SUB",
        "the chosen gate stays"
    );
    assert_eq!(app.time.at, 1, "importance goes back to the last time");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_values_table_shows_quantiles_only_when_sampling() {
    let mut app = pumps();
    app.time.samples = 1;
    let mut h = harness_for(app);
    h.state_mut().run_time_analysis();
    h.run();
    h.get_by_label_contains("Gate NO_FLOW");
    assert!(h.query_by_label_contains("q 5%").is_none());
    assert!(h.query_by_label_contains("means of").is_none());
    h.state_mut().time.samples = 2;
    h.state_mut().run_time_analysis();
    h.run();
    assert!(h.query_by_label_contains("q 5%").is_some());
    // The chart names what it shows.
    h.get_by_label_contains("chart of probability q from time 0 to 5000");
    // One finite time: no chart.
    h.state_mut().time.times = "7".into();
    h.state_mut().run_time_analysis();
    h.run();
    h.get_by_label_contains("needs two or more finite times");
    assert!(h.query_by_label_contains("chart of").is_none());
}

fn square() -> Rect {
    Rect::from_min_max(pos2(0.0, 0.0), pos2(100.0, 100.0))
}

#[test]
fn chart_shape_maps_time_and_value_into_the_area() {
    assert!(chart_shape(&[(1.0, 0.5, 0.5, 0.5)], square(), false).is_none());
    let pts = [
        (0.0, 0.0, 0.0, 0.0),
        (5.0, 0.25, 0.2, 0.5),
        (10.0, 0.5, 0.4, 1.0),
    ];
    let c = chart_shape(&pts, square(), false).unwrap();
    assert_eq!(
        (c.t0, c.t1, c.ymax),
        (0.0, 10.0, 0.5),
        "the top is the largest mean"
    );
    assert_eq!(
        c.line,
        [pos2(0.0, 100.0), pos2(50.0, 50.0), pos2(100.0, 0.0)]
    );
    assert!(c.band.is_empty());
    // Sampled: the top also covers the 95% values, and the band goes up the
    // 95% values and back down the 5% ones.
    let c = chart_shape(&pts, square(), true).unwrap();
    assert_eq!(c.ymax, 1.0);
    assert_eq!(c.line[1], pos2(50.0, 75.0));
    assert_eq!(
        c.band,
        [
            pos2(0.0, 100.0),
            pos2(50.0, 50.0),
            pos2(100.0, 0.0),
            pos2(100.0, 60.0),
            pos2(50.0, 80.0),
            pos2(0.0, 100.0)
        ]
    );
    // An offset area, all-zero values (top 1), and the same time twice.
    let area = Rect::from_min_max(pos2(10.0, 20.0), pos2(30.0, 60.0));
    let c = chart_shape(
        &[(2.0, 0.0, 0.0, 0.0), (2.0, f64::NAN, 0.0, 0.0)],
        area,
        false,
    )
    .unwrap();
    assert_eq!(c.ymax, 1.0);
    assert_eq!(c.line, [pos2(10.0, 60.0), pos2(10.0, 60.0)]);
    let c = chart_shape(&[(1.0, 2.0, 0.0, 0.0), (3.0, 1.0, 0.0, 0.0)], area, false).unwrap();
    assert_eq!(c.line, [pos2(10.0, 20.0), pos2(30.0, 40.0)]);
}

#[test]
fn the_band_is_drawn_only_when_sampling() {
    let area = Rect::from_min_max(pos2(0.0, 0.0), pos2(100.0, 100.0));
    for (samples, band) in [(1, false), (2, true)] {
        let mut app = pumps();
        app.time.samples = samples;
        app.run_time_analysis();
        let r = app.time.report.clone().unwrap();
        let c = ftar_gui::chart_for(&r, &r.gates[0].series, Curve::Probability, area).unwrap();
        assert_eq!(!c.band.is_empty(), band, "{samples} samples");
        assert_eq!(c.line.len(), 4);
    }
}
