//! "Constant Failure Rate/Unit Time" (#123): the flag changes the analysis
//! results through the unit time, and each rate checkbox shows the
//! probability the rate gives.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use eframe::egui::vec2;
use egui_kittest::{kittest::Queryable, Harness};
use ftar_gui::{App, Tab};

const DATA: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../crates/core/tests/data");

/// theatre.fta with GEN_FAIL (0.02) set as a failure rate or a probability.
fn theatre(rate: bool) -> App {
    // a copy: running an analysis writes its report next to the tree
    static N: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "ftar_gui_rate_{}_{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    for f in ["theatre.fta", "theatre.ped"] {
        std::fs::copy(PathBuf::from(DATA).join(f), dir.join(f)).unwrap();
    }
    let mut app = App::default();
    app.open(dir.join("theatre.fta"));
    app.event_form_load("GEN_FAIL");
    app.event_form.rate = rate;
    assert!(app.event_form_apply(), "{:?}", app.error);
    app
}

fn top_probability(rate: bool, unit_time: f64) -> f64 {
    let mut app = theatre(rate);
    app.tab = Tab::Probability;
    app.unit_time = unit_time;
    app.run_analysis();
    assert!(app.error.is_none(), "{:?}", app.error);
    app.prob.as_ref().unwrap().top
}

fn monte_carlo(rate: bool, unit_time: f64) -> f64 {
    let mut app = theatre(rate);
    app.tab = Tab::MonteCarlo;
    app.unit_time = unit_time;
    app.trials = 200_000;
    app.seed = 7;
    app.run_analysis();
    assert!(app.error.is_none(), "{:?}", app.error);
    app.mc.as_ref().unwrap().probability
}

/// Selection path of the tree node `id` (searched from `prefix` down).
fn path_of(app: &App, id: &str, prefix: Vec<usize>) -> Option<Vec<usize>> {
    let mut probe = App::default();
    probe.doc = app.doc.clone();
    for i in 0..8 {
        let mut p = prefix.clone();
        p.push(i);
        probe.select(Some(p.clone()));
        let Some(n) = probe.selected_node() else {
            break;
        };
        if n.id == id {
            return Some(p);
        }
        if let Some(found) = path_of(app, id, p) {
            return Some(found);
        }
    }
    None
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-12 * b.abs()
}

#[test]
fn failure_rate_flag_changes_the_probability_analysis() {
    // cut sets {GEN, MAINS} and {MAINS, RELAY}: MAINS 0.03, RELAY 0.05
    let exact = |gen: f64| 0.03 * gen + 0.0015 - 0.03 * 0.05 * gen;
    let off = top_probability(false, 100.0);
    assert!(close(off, exact(0.02)), "{off}");
    assert!(close(top_probability(false, 1.0), off), "unit time ignored");
    let on = top_probability(true, 100.0);
    assert!(close(on, exact(1.0 - (-0.02f64 * 100.0).exp())), "{on}");
    assert!(close(
        top_probability(true, 1.0),
        exact(1.0 - (-0.02f64).exp())
    ));
}

#[test]
fn failure_rate_flag_changes_monte_carlo() {
    let off = monte_carlo(false, 100.0);
    let on = monte_carlo(true, 100.0);
    // exact: 2.07e-3 without the rate, 2.61e-2 with it
    assert!((1.5e-3..3e-3).contains(&off), "{off}");
    assert!((2.2e-2..3e-2).contains(&on), "{on}");
}

#[test]
fn rate_checkboxes_show_the_probability_at_the_unit_time() {
    let mut app = theatre(true);
    app.unit_time = 100.0;
    assert_eq!(
        app.rate_hint("0.02", true).as_deref(),
        Some("probability 0.864665 at unit time 100")
    );
    assert_eq!(app.rate_hint("0.02", false), None);
    assert_eq!(app.rate_hint("x", true), None);
    assert_eq!(app.rate_hint("-1", true), None);

    // event editor window and primary event database window
    app.events_open = true;
    app.ped.open = true;
    assert!(app.ped_open(&PathBuf::from(DATA).join("theatre.ped")));
    app.ped_select("GEN_FAIL");
    let mut h = Harness::builder()
        .with_size(vec2(1600.0, 1000.0))
        .build_eframe(move |_| app);
    h.run();
    assert_eq!(
        h.get_all_by_label("probability 0.864665 at unit time 100")
            .count(),
        2
    );

    // the details panel of the selected tree event
    let probe = theatre(true);
    let path = path_of(&probe, "GEN_FAIL", vec![]).expect("GEN_FAIL in theatre.fta");
    h.state_mut().events_open = false;
    h.state_mut().ped.open = false;
    h.state_mut().select(Some(path));
    h.run();
    h.run();
    assert_eq!(
        h.get_all_by_label("probability 0.864665 at unit time 100")
            .count(),
        1
    );

    h.state_mut().events_open = true;
    h.state_mut().ped.open = true;
    h.state_mut().select(None);
    h.state_mut().event_form.rate = false;
    h.state_mut().ped.form.rate = false;
    h.run();
    assert_eq!(
        h.query_all_by_label("probability 0.864665 at unit time 100")
            .count(),
        0
    );
}
