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

fn fresh_dir(name: &str) -> std::path::PathBuf {
    let d = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// A tree using database `db` (or none) with a primary event `a` and an
/// intermediate event `x` under an OR gate.
fn tree_file(db: &str, events: &[(&str, char)]) -> String {
    let mut s = format!(
        "{db}\nS NULL 0\n0\nM NULL 1 \n0\nO NULL {} \n",
        events.len()
    );
    for (id, k) in events {
        s.push_str(&format!("{k} {id} 0 \n"));
    }
    s
}

#[test]
fn trees_using_an_event_share_its_database() {
    use ftar_core::project::{trees_using_event, LoadOptions};
    let dir = fresh_dir("event_usage");
    let w = |f: &str, s: String| std::fs::write(dir.join(f), s).unwrap();
    w(
        "a.fta",
        tree_file("shared.ped", &[("PUMP", 'B'), ("VALVE", 'B')]),
    );
    w(
        "b.fta",
        tree_file("shared.ped", &[("PUMP", 'U'), ("PUMP", 'E')]),
    );
    w("c.fta", tree_file("shared.ped", &[("VALVE", 'B')]));
    // same id, other database: a different event
    w("d.fta", tree_file("other.ped", &[("PUMP", 'B')]));
    // same id on an intermediate event: not a use of the primary event
    w(
        "e.fta",
        "shared.ped\nS NULL 0\n0\nM PUMP 1 \n0\nO NULL 1 \nB VALVE 0 \n".into(),
    );
    w("broken.fta", "shared.ped\nnot a tree\n".into());
    w("notes.txt", "PUMP".into());
    std::fs::write(dir.join("shared.ped"), "").unwrap();

    let u = trees_using_event(&dir, "PUMP", Some("shared.ped"), &LoadOptions::default()).unwrap();
    let got: Vec<_> = u
        .trees
        .iter()
        .map(|t| (t.path.file_name().unwrap().to_str().unwrap(), t.nodes))
        .collect();
    assert_eq!(got, [("a.fta", 1), ("b.fta", 2)]);
    assert_eq!(u.skipped, [dir.join("broken.fta")]);
    assert!(!u.truncated);

    let other =
        trees_using_event(&dir, "PUMP", Some("other.ped"), &LoadOptions::default()).unwrap();
    assert_eq!(other.trees.len(), 1);
    assert!(
        trees_using_event(&dir, "PUMP", None, &LoadOptions::default())
            .unwrap()
            .trees
            .is_empty()
    );
    assert!(
        trees_using_event(&dir, "NOPE", Some("shared.ped"), &LoadOptions::default())
            .unwrap()
            .trees
            .is_empty()
    );
}

#[test]
fn trees_without_a_database_match_each_other() {
    use ftar_core::project::{trees_using_event, LoadOptions};
    let dir = fresh_dir("event_usage_none");
    std::fs::write(dir.join("a.fta"), tree_file("NULL", &[("X", 'B')])).unwrap();
    std::fs::write(dir.join("b.fta"), tree_file("own.ped", &[("X", 'B')])).unwrap();
    let u = trees_using_event(&dir, "X", None, &LoadOptions::default()).unwrap();
    assert_eq!(u.trees.len(), 1);
    assert_eq!(u.trees[0].path, dir.join("a.fta"));
}

#[test]
fn event_usage_stays_inside_the_root() {
    use ftar_core::project::{trees_using_event, LoadOptions};
    let base = fresh_dir("event_usage_root");
    let inside = base.join("inside");
    std::fs::create_dir_all(&inside).unwrap();
    std::fs::write(inside.join("a.fta"), tree_file("s.ped", &[("X", 'B')])).unwrap();
    let opts = LoadOptions::confined(&inside).unwrap();
    assert_eq!(
        trees_using_event(&inside, "X", Some("s.ped"), &opts)
            .unwrap()
            .trees
            .len(),
        1
    );
    assert!(trees_using_event(&base, "X", Some("s.ped"), &opts).is_err());
    assert!(trees_using_event(&base.join("missing"), "X", None, &LoadOptions::default()).is_err());
}
