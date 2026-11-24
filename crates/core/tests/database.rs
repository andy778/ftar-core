#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
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

#[test]
fn export_text_follows_openped_layout() {
    use ftar_core::database::export_text;
    let mut db = ftar_core::PrimaryEventDb::default();
    let mut b = event("B");
    b.modifier = Some('D');
    b.lambda = Some('L');
    db.events.insert("B".into(), b);
    db.events.insert("A".into(), event("A"));
    let t = export_text(&db, "");
    assert!(t.starts_with(
        "Primary Events Database\n=======================\n\nDatabase Filename: Unnamed\n\nID: A\n"
    ));
    assert!(t.contains("Probability: 0.5\nType: Basic\nDormant: No\nDescription: d\n\nID: B\n"));
    assert!(t.ends_with(
        "Probability: 0.5 (Constant failure rate/Unit time)\nType: Basic\nDormant: Yes\nDescription: d"
    ));
    assert!(export_text(&db, "x.ped").contains("Filename: x.ped\n"));
}

#[test]
fn copy_events_between_databases() {
    use ftar_core::database::copy_events;
    let mut src = ftar_core::PrimaryEventDb::default();
    src.events.insert("A".into(), event("A"));
    src.events.insert("B".into(), event("B"));
    let mut dst = ftar_core::PrimaryEventDb::default();
    let mut old = event("A");
    old.probability = 0.1;
    dst.events.insert("A".into(), old);
    let ids: Vec<String> = ["A", "B", "Z"].iter().map(|s| s.to_string()).collect();
    let o = copy_events(&src, &mut dst, &ids, false).unwrap();
    assert_eq!(o.copied, vec!["B".to_string()]);
    assert_eq!(o.skipped, vec!["A".to_string()]);
    assert_eq!(o.missing, vec!["Z".to_string()]);
    assert_eq!(dst.events["A"].probability, 0.1);
    let o = copy_events(&src, &mut dst, &ids, true).unwrap();
    assert_eq!(o.copied.len(), 2);
    assert_eq!(dst.events["A"].probability, 0.5);
    // an invalid event leaves the destination untouched
    src.events.get_mut("B").unwrap().probability = 7.0;
    let mut d2 = ftar_core::PrimaryEventDb::default();
    assert!(copy_events(&src, &mut d2, &ids, true).is_err());
    assert!(d2.events.is_empty());
}

#[test]
fn link_node_takes_id_and_type_and_asks_when_types_differ() {
    use ftar_core::database::{link_node, LinkOutcome};
    let mut p = lift();
    let leaf = ftar_core::edit::all_paths(&p.tree)
        .into_iter()
        .find(|q| {
            ftar_core::edit::node_at(&p.tree, q)
                .is_some_and(|n| n.children.is_empty() && n.kind == EventType::Basic)
        })
        .unwrap();
    let mut ev = event("NEWEV");
    ev.kind = EventType::External;
    p.db.events.insert("NEWEV".into(), ev);
    assert_eq!(
        link_node(&mut p.tree, &p.db, &leaf, "NEWEV", false).unwrap(),
        LinkOutcome::TypesDiffer
    );
    assert_ne!(
        ftar_core::edit::node_at(&p.tree, &leaf).unwrap().id,
        "NEWEV"
    );
    assert_eq!(
        link_node(&mut p.tree, &p.db, &leaf, "NEWEV", true).unwrap(),
        LinkOutcome::Linked
    );
    let n = ftar_core::edit::node_at(&p.tree, &leaf).unwrap();
    assert_eq!((n.id.as_str(), n.kind), ("NEWEV", EventType::External));
    assert!(link_node(&mut p.tree, &p.db, &leaf, "NOPE", true).is_err());
    assert!(link_node(&mut p.tree, &p.db, &[], "NEWEV", true).is_err());
    assert!(link_node(&mut p.tree, &p.db, &[99, 99], "NEWEV", true).is_err());
}
