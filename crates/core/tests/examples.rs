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

mod first_failure {
    use ftar_core::analysis::{probability_report, ProbabilityOptions};
    use ftar_core::montecarlo::{simulate, Method, MonteCarloOptions};
    use ftar_core::project::Project;

    fn theatre() -> Project {
        let d = format!("{}/tests/data/theatre.fta", env!("CARGO_MANIFEST_DIR"));
        Project::load(d).unwrap()
    }

    fn run(
        p: &Project,
        method: Method,
        trials: u64,
        seed: u64,
    ) -> ftar_core::montecarlo::MonteCarloReport {
        let o = MonteCarloOptions {
            trials,
            seed,
            method,
            ..Default::default()
        };
        simulate(&p.tree, &p.db, &o).unwrap()
    }

    #[test]
    fn modes_account_for_every_system_failure() {
        let p = theatre();
        let r = run(&p, Method::FirstFailure, 50_000, 3);
        assert_eq!(r.modes.iter().map(|m| m.1).sum::<u64>(), r.system_failures);
        assert!((r.bas_prob - r.any_component_failure).abs() < 1e-15);
        assert!(r.modes.windows(2).all(|w| w[0].1 >= w[1].1));
        // every recorded mode really fails the system: it contains a cut set
        let cs = ftar_core::analysis::minimal_cut_sets(&p.tree).unwrap();
        for (m, _) in &r.modes {
            assert!(cs.iter().any(|c| c.is_subset(m)), "{m:?}");
        }
    }

    #[test]
    fn first_failure_is_unbiased_and_less_noisy_than_direct() {
        let p = theatre();
        let exact = probability_report(&p.tree, &p.db, &ProbabilityOptions::default())
            .unwrap()
            .top;
        let trials = 20_000;
        let (mut se_first, mut se_direct, mut err_first) = (0.0, 0.0, 0.0);
        let runs = 40;
        for seed in 0..runs {
            let f = run(&p, Method::FirstFailure, trials, seed);
            let d = run(&p, Method::Direct, trials, seed);
            err_first += f.probability - exact;
            se_first += (f.probability - exact).powi(2);
            se_direct += (d.probability - exact).powi(2);
        }
        let mean_err = err_first / runs as f64;
        let rmse_first = (se_first / runs as f64).sqrt();
        assert!(
            mean_err.abs() < 3.0 * rmse_first / (runs as f64).sqrt(),
            "bias {mean_err}"
        );
        assert!(
            rmse_first < (se_direct / runs as f64).sqrt(),
            "first-failure should have lower variance"
        );
    }

    #[test]
    fn all_zero_probabilities_abandon_the_analysis() {
        let mut p = theatre();
        for e in p.db.events.values_mut() {
            e.probability = 0.0;
        }
        let r = run(&p, Method::FirstFailure, 1000, 1);
        assert_eq!(
            (r.system_failures, r.probability, r.any_component_failure),
            (0, 0.0, 0.0)
        );
    }
}

mod document {
    use ftar_core::analysis::minimal_cut_sets;
    use ftar_core::project::Document;

    fn data(n: &str) -> String {
        format!("{}/tests/data/{n}", env!("CARGO_MANIFEST_DIR"))
    }

    #[test]
    fn keeps_transfers_raw_but_analyses_expanded() {
        let d = Document::load(data("top.fta")).unwrap();
        assert!(d
            .tree
            .nodes()
            .iter()
            .any(|n| n.kind == ftar_core::EventType::TransferIn));
        let p = d.analysis_project().unwrap();
        assert_eq!(minimal_cut_sets(&p.tree).unwrap().len(), 2);
        assert_eq!(p.db.events.len(), 2);
    }

    #[test]
    fn default_tree_is_valid_to_edit() {
        let d = ftar_core::project::Document::default();
        assert_eq!(d.tree.root.children.len(), 1);
    }
}
