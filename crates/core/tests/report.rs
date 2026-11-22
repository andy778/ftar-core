use ftar_core::analysis::{minimal_cut_sets, probability_report, CutSet, ProbabilityOptions};
use ftar_core::montecarlo::{simulate, MonteCarloOptions};
use ftar_core::project::Project;
use ftar_core::report::*;

const TIME: &str = "Mon Jan  1 12:00:00 2024";

fn data(name: &str) -> String {
    format!("{}/tests/data/{name}", env!("CARGO_MANIFEST_DIR"))
}

fn set(ids: &[&str]) -> CutSet {
    ids.iter().map(|s| s.to_string()).collect()
}

#[test]
fn fmt_e_matches_c() {
    assert_eq!(fmt_e(0.0), "0.000000E+00");
    assert_eq!(fmt_e(0.28), "2.800000E-01");
    assert_eq!(fmt_e(1.5e-120), "1.500000E-120");
    assert_eq!(fmt_e(12345.0), "1.234500E+04");
}

#[test]
fn cut_set_report_layout() {
    let p = Project::load(data("theatre.fta")).unwrap();
    let cs = minimal_cut_sets(&p.tree).unwrap();
    let ev = primary_event_order(&p.tree);
    let r = cut_sets_report("theatre.fta", TIME, &ev, &cs);
    assert!(r.starts_with(
        "Minimal Cut Sets\n================\n\nTree   : theatre.fta\nTime   : Mon Jan  1 12:00:00 2024\n\nMethod : Algebraic\n\n"
    ));
    assert!(r.contains("No. of primary events = 3\nMinimal cut set order = 1 to 2\n"));
    assert!(r.contains("\nOrder 1:\n\nOrder 2:\n  "), "{r}");
    assert!(r.contains(
        "Qualitative Importance Analysis:\n\nOrder        Number\n-----        ------\n"
    ));
    assert!(
        r.contains("   1           0\n   2           2\n  ALL          2\n"),
        "{r}"
    );
}

#[test]
fn probability_report_layout_and_values() {
    let p = Project::load(data("theatre.fta")).unwrap();
    let opts = ProbabilityOptions::default();
    let rep = probability_report(&p.tree, &p.db, &opts).unwrap();
    let ev = primary_event_order(&p.tree);
    let r = probability_report_text("theatre.fta", TIME, &ev, 2, &opts, &rep);
    assert!(r.starts_with("Probabilities Analysis\n======================\n\n"));
    assert!(r.contains(
        "Number of minimal cut sets = 2\nOrder of minimal cut sets  = 2\n\nUnit time span         = 1.000000\n\n"
    ));
    assert!(r.contains("Minimal cut set probabilities :\n\n  1   "));
    assert!(r.contains("(minimal cut sets up to order 2 used):"));
    assert!(r.contains(" 1 term    +"));
    assert!(r.contains(" 2 terms   -"));
    assert!(r.contains("(upper bound)") && r.contains("(lower bound)"));
    assert!(r.contains(&format!("\nExact value : {}\n", fmt_e(rep.top))));
    assert!(
        r.contains("Primary Event Analysis:\n\n Event          Failure contrib.    Importance\n\n")
    );
    assert!(r.contains(" MAINS_FAIL"));
}

#[test]
fn probability_report_order_limit_note() {
    let p = Project::load(data("theatre.fta")).unwrap();
    let opts = ProbabilityOptions {
        max_order: 1,
        ..Default::default()
    };
    // order-1 limit leaves no cut sets for theatre, so use tree max order 2 with a fake report
    let rep = ftar_core::analysis::ProbabilityReport {
        cut_sets: vec![],
        terms: vec![],
        partial_sums: vec![],
        top: 0.0,
        exact: true,
        events: vec![],
    };
    let r = probability_report_text("t", TIME, &primary_event_order(&p.tree), 2, &opts, &rep);
    assert!(r.contains("               (order <= 1 used)\n\n"));
}

#[test]
fn rank_list_swaps_only_on_strictly_greater() {
    let mut fs = FailureStatistics::new(0);
    let (a, b, c) = (set(&["a"]), set(&["b"]), set(&["c"]));
    fs.record(&a);
    fs.record(&b);
    fs.record(&c);
    // all 1: newest at the low end, so oldest is ranked first
    let names: Vec<_> = fs.ranked().iter().map(|m| m.events.clone()).collect();
    assert_eq!(names, vec![a.clone(), b.clone(), c.clone()]);
    fs.record(&c);
    fs.record(&c);
    fs.record(&b);
    // c=3, b=2, a=1
    let r = fs.ranked();
    assert_eq!((r[0].events.clone(), r[0].count), (c, 3));
    assert_eq!((r[1].events.clone(), r[1].count), (b, 2));
    assert_eq!((r[2].events.clone(), r[2].count), (a, 1));
}

#[test]
fn max_fail_limits_modes() {
    let mut fs = FailureStatistics::new(2);
    for id in ["a", "b", "c", "d", "e"] {
        fs.record(&set(&[id]));
    }
    // C quirk: max_fail + 1 modes are kept
    assert_eq!(fs.len(), 3);
    assert_eq!(fs.other(), 2);
}

#[test]
fn compress_absorbs_supersets() {
    let mut fs = FailureStatistics::new(0);
    fs.record_n(&set(&["a"]), 5);
    fs.record_n(&set(&["a", "b"]), 3);
    fs.record_n(&set(&["b", "c"]), 2);
    fs.record_n(&set(&["a", "b", "c"]), 1);
    let c = fs.compress();
    let got: Vec<_> = c
        .iter()
        .map(|m| {
            (
                m.events.iter().cloned().collect::<Vec<_>>().join(""),
                m.count,
            )
        })
        .collect();
    // {a} absorbs {a,b} (+3) and {a,b,c} (+1); {b,c} absorbs {a,b,c} (+1)
    assert_eq!(got, vec![("a".to_string(), 9), ("bc".to_string(), 3)]);
}

#[test]
fn monte_carlo_report_layout() {
    let p = Project::load(data("theatre.fta")).unwrap();
    let mc = simulate(
        &p.tree,
        &p.db,
        &MonteCarloOptions {
            trials: 200_000,
            ..Default::default()
        },
    )
    .unwrap();
    let fs = failure_statistics_from_hits(&mc, 0);
    let r = monte_carlo_report_text("theatre.fta", TIME, 1.0, &mc, &fs);
    assert!(r.starts_with("Monte Carlo Simulation\n======================\n\n"));
    assert!(r.contains("Number of tests           = 200000\n"));
    assert!(r.contains("Unit Time span used       = 1.000000\n\n"));
    assert!(r.contains(&format!(
        "Number of system failures = {}\n\n",
        mc.system_failures
    )));
    assert!(r.contains("Importance\n\n  1   "), "{r}");
    assert!(r.contains("\n\nCompressed:\n\n"));
    assert!(r.contains("\n\nPrimary Event Analysis:\n\n"));
    assert!(r.contains(&format!(
        "Probability of top event  = {}",
        fmt_e(mc.probability)
    )));
}

/// An event in no failure mode contributes 0, printed like OpenFTA as
/// `0.000000E+00` / `0.00%`, never `-0.000000E+00` / `-0.00%`.
#[test]
fn unused_event_contributes_positive_zero() {
    let p = Project::load(data("lift.fta")).unwrap();
    let mc = simulate(
        &p.tree,
        &p.db,
        &MonteCarloOptions {
            trials: 2_000,
            ..Default::default()
        },
    )
    .unwrap();
    let fs = failure_statistics_from_hits(&mc, 0);
    let r = monte_carlo_report_text("lift.fta", TIME, 1.0, &mc, &fs);
    assert!(!r.contains("-0.0"), "{r}");
    assert!(
        r.contains(" W_1            0.000000E+00             0.00%\n"),
        "{r}"
    );
    let c = event_contributions(&["W_1".to_string()], &[], 1, 1.0, 1.0);
    assert!(c[0].1.is_sign_positive() && c[0].2.is_sign_positive());
}

#[test]
fn failure_statistics_report_lists_modes() {
    let mut fs = FailureStatistics::new(0);
    fs.record(&set(&["x", "y"]));
    fs.record(&set(&["x", "y"]));
    let r = failure_statistics_report("t.fta", TIME, &fs);
    assert!(r.contains("Number of failure modes   = 1"));
    assert!(r.contains("(0  ) x y - 2"));
}

#[test]
fn top_with_transfers_reports() {
    let p = Project::load(data("top.fta")).unwrap();
    let cs = minimal_cut_sets(&p.tree).unwrap();
    let r = cut_sets_report("top.fta", TIME, &primary_event_order(&p.tree), &cs);
    assert!(r.contains("  ALL          2\n"));
}

/// Reported by a tester: OpenFTA prints every term and `Exact value : 1.199993E-05`
/// for lift.fta (12 cut sets); the default options must do the same.
#[test]
fn default_probability_report_has_exact_value_for_lift() {
    let p = Project::load(data("lift.fta")).unwrap();
    let opts = ProbabilityOptions::default();
    let rep = probability_report(&p.tree, &p.db, &opts).unwrap();
    assert!(rep.exact);
    assert_eq!(rep.terms.len(), 12);
    let ev = primary_event_order(&p.tree);
    let r = probability_report_text("lift.fta", TIME, &ev, 1, &opts, &rep);
    assert!(r.contains("\nExact value : 1.199993E-05\n"), "{r}");
    assert!(r.contains("12 terms"), "{r}");
}

/// Reported by a tester: for lift.fta (14 primary events, all cut sets of
/// order 1) OpenFTA prints the order limit, 14, not the largest order found.
#[test]
fn probability_report_prints_the_order_limit_like_openfta() {
    let p = Project::load(data("lift.fta")).unwrap();
    let r = generate(
        ReportKind::Probability,
        &p.tree,
        &p.db,
        "lift.fta",
        TIME,
        &ReportParams::default(),
    )
    .unwrap();
    assert!(r.contains("Number of primary events   = 14\n"), "{r}");
    assert!(
        r.contains("\nOrder of minimal cut sets  = 14\n\nUnit time"),
        "{r}"
    );
    assert!(r.contains("(minimal cut sets up to order 14 used):"), "{r}");

    // a chosen limit is printed as is
    let params = ReportParams {
        max_order: 2,
        ..ReportParams::default()
    };
    let r = generate(
        ReportKind::Probability,
        &p.tree,
        &p.db,
        "lift.fta",
        TIME,
        &params,
    )
    .unwrap();
    assert!(
        r.contains("\nOrder of minimal cut sets  = 2\n\nUnit time"),
        "{r}"
    );
    assert!(r.contains("(minimal cut sets up to order 2 used):"), "{r}");
}

/// OpenFTA lists primary events alphabetically (`strcmp`, `event_list.c`),
/// not in tree order. lift.fta has them out of order (LMD_1 comes first).
#[test]
fn primary_events_are_listed_in_openfta_order() {
    let p = Project::load(data("lift.fta")).unwrap();
    let first_in_tree = p
        .tree
        .nodes()
        .into_iter()
        .find(|n| n.kind.is_primary())
        .unwrap();
    assert_eq!(first_in_tree.id, "LMD_1");

    let ev = primary_event_order(&p.tree);
    let mut sorted = ev.clone();
    sorted.sort();
    assert_eq!(ev, sorted);
    // byte order, as strcmp: `S` sorts before `_`
    let pos = |id: &str| ev.iter().position(|e| e == id).unwrap();
    assert!(pos("DMS_1") < pos("DM_1"));

    let rep = probability_report(&p.tree, &p.db, &ProbabilityOptions::default()).unwrap();
    let ids: Vec<_> = rep.events.iter().map(|e| e.id.clone()).collect();
    assert_eq!(ids, ev);

    let mc = simulate(
        &p.tree,
        &p.db,
        &MonteCarloOptions {
            trials: 100,
            ..Default::default()
        },
    )
    .unwrap();
    let ids: Vec<_> = mc.event_failures.iter().map(|(id, _)| id.clone()).collect();
    assert_eq!(ids, ev);
}

/// Within a cut set line the events follow the same order. theatre.fta has
/// RELAY_FAIL before MAINS_FAIL in the tree.
#[test]
fn cut_set_lines_list_events_alphabetically() {
    let p = Project::load(data("theatre.fta")).unwrap();
    let cs = minimal_cut_sets(&p.tree).unwrap();
    let r = cut_sets_report("theatre.fta", TIME, &primary_event_order(&p.tree), &cs);
    assert!(r.contains(" MAINS_FAIL RELAY_FAIL"), "{r}");
    assert!(!r.contains(" RELAY_FAIL MAINS_FAIL"), "{r}");
}
