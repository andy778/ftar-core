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
