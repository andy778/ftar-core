use ftar_core::database::{remove_event, upsert_event, validate_event};
use ftar_core::project::Project;
use ftar_core::{EventType, PrimaryEvent};

fn lift() -> Project {
    Project::load(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/lift.fta")).unwrap()
}

fn event(id: &str) -> PrimaryEvent {
    PrimaryEvent {
        id: id.into(),
        group: String::new(),
        kind: EventType::Basic,
        modifier: None,
        description: "d".into(),
        probability: 0.5,
        lambda: None,
    }
}

#[test]
fn upsert_adds_and_replaces_and_retypes_tree_nodes() {
    let mut p = lift();
    let mut ev = event("DOD_1");
    ev.kind = EventType::External;
    ev.probability = 2e-6;
    upsert_event(&mut p.tree, &mut p.db, ev).unwrap();
    assert_eq!(p.db.events["DOD_1"].probability, 2e-6);
    let n = p
        .tree
        .nodes()
        .into_iter()
        .find(|n| n.id == "DOD_1")
        .unwrap();
    assert_eq!(n.kind, EventType::External);
    let before = p.db.events.len();
    upsert_event(&mut p.tree, &mut p.db, event("NEW_1")).unwrap();
    assert_eq!(p.db.events.len(), before + 1);
}

#[test]
fn invalid_events_are_refused_and_change_nothing() {
    let mut p = lift();
    let snapshot = p.db.clone();
    let mut bad = Vec::new();
    for id in ["", "a b", "a;b"] {
        bad.push(event(id));
    }
    let mut e = event("X");
    e.description = "a;b".into();
    bad.push(e);
    let mut e = event("X");
    e.description = "two\nlines".into();
    bad.push(e);
    let mut e = event("X");
    e.probability = 1.5;
    bad.push(e);
    let mut e = event("X");
    e.probability = f64::NAN;
    bad.push(e);
    let mut e = event("X");
    e.kind = EventType::And;
    bad.push(e);
    let mut e = event("X");
    e.modifier = Some('Q');
    bad.push(e);
    for e in bad {
        assert!(validate_event(&e).is_err(), "{e:?}");
        assert!(upsert_event(&mut p.tree, &mut p.db, e).is_err());
    }
    assert_eq!(p.db, snapshot);
    // a failure rate may exceed 1
    let mut r = event("RATE");
    r.probability = 3.0;
    r.lambda = Some('L');
    assert!(validate_event(&r).is_ok());
}

#[test]
fn remove_reports_nodes_still_using_the_event() {
    let mut p = lift();
    let used = remove_event(&p.tree, &mut p.db, "DM_1").unwrap();
    assert!(used >= 1);
    assert!(!p.db.events.contains_key("DM_1"));
    assert_eq!(remove_event(&p.tree, &mut p.db, "DM_1"), None);
}
