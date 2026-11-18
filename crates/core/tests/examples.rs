use ftar_core::analysis::{minimal_cut_sets, top_probability};
use ftar_core::parse::{parse_fta, parse_ped};

fn load(name: &str) -> (ftar_core::Tree, ftar_core::PrimaryEventDb) {
    let d = format!("{}/tests/data/{name}", env!("CARGO_MANIFEST_DIR"));
    let t = parse_fta(&std::fs::read(format!("{d}.fta")).unwrap()).unwrap();
    let p = parse_ped(&std::fs::read(format!("{d}.ped")).unwrap()).unwrap();
    (t, p)
}

#[test]
fn lift_parses() {
    let (t, p) = load("lift");
    assert_eq!(t.db_name.as_deref(), Some("lift.ped"));
    assert_eq!(p.events.len(), 14);
    assert!(t.nodes().len() > 20);
}

#[test]
fn theatre_note_and_cut_sets() {
    let (t, p) = load("theatre");
    assert!(t.note.starts_with("Failure Of An Electrical Supply"));
    let cs = minimal_cut_sets(&t).unwrap();
    // TOP = MAINS_FAIL-ish AND ...; every cut set must reference known events
    assert!(!cs.is_empty());
    for s in &cs {
        for id in s {
            assert!(p.events.contains_key(id), "unknown event {id}");
        }
    }
    let prob = top_probability(&cs, &p).unwrap();
    assert!(prob.rare_event > 0.0 && prob.rare_event >= prob.min_cut_upper_bound);
    println!("{cs:?} {prob:?}");
}

#[test]
fn lift_cut_sets_are_minimal() {
    let (t, _) = load("lift");
    let cs = minimal_cut_sets(&t).unwrap();
    for (i, a) in cs.iter().enumerate() {
        for (j, b) in cs.iter().enumerate() {
            assert!(i == j || !a.is_subset(b), "{a:?} absorbs {b:?}");
        }
    }
}

#[test]
fn transfer_trees_report_unsupported() {
    let d = format!("{}/tests/data/top.fta", env!("CARGO_MANIFEST_DIR"));
    let t = parse_fta(&std::fs::read(d).unwrap()).unwrap();
    assert!(minimal_cut_sets(&t).is_err());
}

use ftar_core::analysis::{probability_report, ProbabilityOptions};
use ftar_core::montecarlo::{simulate, MonteCarloOptions};
use ftar_core::project::Project;

fn data(name: &str) -> String {
    format!("{}/tests/data/{name}", env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn transfer_in_trees_are_inlined() {
    let p = Project::load(data("top.fta")).unwrap();
    let cs = minimal_cut_sets(&p.tree).unwrap();
    let names: Vec<_> = cs
        .iter()
        .map(|s| s.iter().cloned().collect::<Vec<_>>())
        .collect();
    assert_eq!(names, vec![vec!["a".to_string()], vec!["b".to_string()]]);
    let r = probability_report(&p.tree, &p.db, &ProbabilityOptions::default()).unwrap();
    // P(a or b) = 0.1 + 0.2 - 0.1*0.2
    assert!((r.top - 0.28).abs() < 1e-12, "{}", r.top);
    assert!(r.exact);
}

#[test]
fn theatre_exact_probability_uses_unit_time_for_rates() {
    let p = Project::load(data("theatre.fta")).unwrap();
    let r = probability_report(&p.tree, &p.db, &ProbabilityOptions::default()).unwrap();
    // GEN_FAIL is a failure rate: 1 - e^-0.02
    let gen = 1.0 - (-0.02f64).exp();
    let (m, rl) = (0.03, 0.05);
    let exact = gen * m + m * rl - gen * m * rl;
    assert!((r.top - exact).abs() < 1e-12);
    assert_eq!(r.terms.len(), 2);
}

#[test]
fn order_limit_drops_larger_cut_sets() {
    let p = Project::load(data("theatre.fta")).unwrap();
    assert!(ftar_core::analysis::minimal_cut_sets_limited(&p.tree, 1)
        .unwrap()
        .is_empty());
}

#[test]
fn monte_carlo_agrees_with_exact() {
    let p = Project::load(data("theatre.fta")).unwrap();
    let exact = probability_report(&p.tree, &p.db, &ProbabilityOptions::default())
        .unwrap()
        .top;
    let mc = simulate(
        &p.tree,
        &p.db,
        &MonteCarloOptions {
            trials: 1_000_000,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(
        (mc.probability - exact).abs() < 5.0 * mc.std_error,
        "{} vs {exact}",
        mc.probability
    );
    let again = simulate(
        &p.tree,
        &p.db,
        &MonteCarloOptions {
            trials: 1_000_000,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(mc.system_failures, again.system_failures); // reproducible
}
