#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! The text format (`ftar_core::text`): round trips with every OpenFTA
//! example, the same analysis results, and the errors for bad input.

use std::path::{Path, PathBuf};

use ftar_core::analysis::{minimal_cut_sets, probability_report, ProbabilityOptions};
use ftar_core::project::{Document, Project};
use ftar_core::text::{parse_text, write_text, MAX_NODES};
use ftar_core::{Error, PrimaryEventDb, Tree};

fn data_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data")
}

/// Every `.fta` among the test data.
fn examples() -> Vec<PathBuf> {
    let mut out = vec![];
    let mut dirs = vec![data_dir()];
    while let Some(d) = dirs.pop() {
        for e in std::fs::read_dir(d).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                dirs.push(p);
            } else if p.extension().is_some_and(|e| e == "fta") {
                out.push(p);
            }
        }
    }
    out.sort();
    assert!(out.len() >= 10, "examples missing: {out:?}");
    out
}

/// OpenFTA wrote a space or NUL where a marker is absent; only `D` and `L` mean something.
fn normalise(db: &PrimaryEventDb) -> PrimaryEventDb {
    let mut db = db.clone();
    for e in db.events.values_mut() {
        e.modifier = e.modifier.filter(|m| *m == 'D');
        e.lambda = e.lambda.filter(|m| *m == 'L');
    }
    db
}

fn round_trip(tree: &Tree, db: &PrimaryEventDb) -> (Tree, PrimaryEventDb) {
    let text = write_text(tree, db).unwrap();
    let (t, d) = parse_text(text.as_bytes()).unwrap();
    // Writing again gives the same text.
    assert_eq!(write_text(&t, &d).unwrap(), text);
    (t, d)
}

#[test]
fn every_example_round_trips() {
    for f in examples() {
        let doc = Document::load(&f).unwrap();
        let (tree, db) = round_trip(&doc.tree, &doc.db);
        assert_eq!(tree.root, doc.tree.root, "{}", f.display());
        assert_eq!(tree.note, doc.tree.note, "{}", f.display());
        assert_eq!(db, normalise(&doc.db), "{}", f.display());
    }
}

#[test]
fn converted_examples_give_the_same_results() {
    // A few inclusion-exclusion terms are enough to compare; all of them
    // take minutes on the larger examples in a debug build.
    let opts = ProbabilityOptions {
        terms: 3,
        ..ProbabilityOptions::default()
    };
    for f in examples() {
        let p = Project::load(&f).unwrap();
        let (tree, db) = round_trip(&p.tree, &p.db);
        assert_eq!(
            minimal_cut_sets(&tree).unwrap(),
            minimal_cut_sets(&p.tree).unwrap(),
            "{}",
            f.display()
        );
        let (Ok(a), Ok(b)) = (
            probability_report(&p.tree, &p.db, &opts),
            probability_report(&tree, &db, &opts),
        ) else {
            continue; // trees whose events have no probability
        };
        assert_eq!(a.top, b.top, "{}", f.display());
        assert_eq!(a.cut_sets, b.cut_sets, "{}", f.display());
    }
}

#[test]
fn hand_written_theatre_matches_the_openfta_files() {
    let dir = data_dir().join("text");
    let fta = Project::load(data_dir().join("theatre.fta")).unwrap();
    let ftt = Project::load(dir.join("theatre.ftt")).unwrap();
    let opts = ProbabilityOptions::default();
    let a = probability_report(&fta.tree, &fta.db, &opts).unwrap();
    let b = probability_report(&ftt.tree, &ftt.db, &opts).unwrap();
    assert_eq!(a.top, b.top);
    assert_eq!(a.cut_sets, b.cut_sets);
    assert_eq!(ftt.db.events["GEN_FAIL"].lambda, Some('L'));
    // A document keeps the database in the same file.
    let doc = Document::load(dir.join("theatre.ftt")).unwrap();
    assert_eq!(doc.tree.db_name, None);
    assert_eq!(doc.db.events.len(), 3);
}

#[test]
fn text_tree_can_transfer_in_an_fta_tree() {
    let p = Project::load(data_dir().join("text/uses_transfer.ftt")).unwrap();
    let sets = minimal_cut_sets(&p.tree).unwrap();
    let names: Vec<String> = sets.iter().map(|s| s.join(" ")).collect();
    assert!(names.contains(&"EXTRA".to_string()), "{names:?}");
    assert!(
        names.contains(&"GEN_FAIL MAINS_FAIL".to_string()),
        "{names:?}"
    );
    assert!(p.db.events.contains_key("RELAY_FAIL"));
}

fn parse(s: &str) -> Result<(Tree, PrimaryEventDb), Error> {
    parse_text(s.as_bytes())
}

fn parse_err(s: &str) -> String {
    parse(s).unwrap_err().to_string()
}

const SMALL: &str = "Gate: T\n- type: OR\n- inputs: A, B\n\nEvent: A\n- probability: 0.1\n\nEvent: B\n- probability: 0.2\n";

#[test]
fn small_tree_parses() {
    let (t, db) = parse(SMALL).unwrap();
    assert_eq!(t.root.id, "T");
    assert_eq!(t.root.children.len(), 1);
    assert_eq!(t.root.children[0].children.len(), 2);
    assert_eq!(db.events.len(), 2);
    // CRLF, indentation and comments are fine.
    let crlf = format!("# c\r\n{}", SMALL.replace('\n', "\r\n  "));
    assert_eq!(parse(&crlf).unwrap(), (t, db));
}

#[test]
fn unnamed_and_untyped_gates() {
    let src = "Gate: x\n- unnamed: true\n- inputs: y\n\nGate: y\n- type: AND\n- gate_id: G1\n- inputs: A, B\n\nEvent: A\n\nEvent: B\n";
    let (t, db) = parse(src).unwrap();
    assert_eq!(t.root.id, "");
    let inner = &t.root.children[0];
    assert_eq!(inner.id, "y");
    assert_eq!(inner.children[0].id, "G1");
    assert!(db.events.is_empty());
}

#[test]
fn bad_input_is_an_error() {
    let cases: &[(&str, &str)] = &[
        ("", "no `Gate:`"),
        ("Gate: T\n- inputs: A\n", "input `A` is not defined"),
        ("Gate: T\n- inputs: T\n", "its own input"),
        (
            "Gate: T\n- inputs: U\n\nGate: U\n- inputs: T\n",
            "its own input",
        ),
        (
            "Gate: T\n- inputs: A\n\nEvent: A\n\nEvent: A\n",
            "defined twice",
        ),
        (
            "Gate: T\n- inputs: A\n\nEvent: A\n\nGate: U\n- inputs: A\n",
            "`U` is not part of the tree",
        ),
        ("Gate: T\n- colour: red\n", "unknown key `colour`"),
        ("Gate: T\n- type: NAND\n", "unknown gate type"),
        ("Gate: T\n- gate_id: G\n", "needs a `type`"),
        ("Gate: T\n- label: a\n- label: b\n", "more than once"),
        ("Gate: T\n- inputs: A,,B\n", "bad id"),
        ("Gate: two words\n", "bad id"),
        ("Gate:\n", "bad id"),
        ("Fault: T\n", "unknown paragraph"),
        ("Gate: T\nlabel\n", "expected"),
        ("Gate: T\n\n- label: x\n", "field outside a paragraph"),
        (
            "Gate: T\n- inputs: A\n\nEvent: A\n- type: dog\n",
            "unknown event type",
        ),
        (
            "Gate: T\n- inputs: A\n\nEvent: A\n- probability: 2\n",
            "probability must be within 0..1",
        ),
        (
            "Gate: T\n- inputs: A\n\nEvent: A\n- probability: x\n",
            "bad number",
        ),
        (
            "Gate: T\n- inputs: A\n\nEvent: A\n- probability: 0.1\n- failure_rate: 1\n",
            "not both",
        ),
        (
            "Gate: T\n- inputs: A\n\nEvent: A\n- label: no value\n",
            "needs a `probability`",
        ),
        (
            "Gate: T\n- inputs: A\n\nEvent: A\n- dormant: maybe\n",
            "`true` or `false`",
        ),
        ("Gate: T\n- id: X\n- unnamed: true\n", "not both"),
        ("Gate: T\n- id: x y\n", "bad id"),
    ];
    for (src, want) in cases {
        let e = parse_err(src);
        assert!(e.contains(want), "{src:?}: got `{e}`, want `{want}`");
    }
    assert!(parse_text(&[0xff, 0xfe]).is_err());
}

#[test]
fn pfta_only_keys_are_reported_as_not_supported_yet() {
    let src = "- times: 1, 2\n\nGate: T\n- inputs: A\n\nEvent: A\n";
    assert!(matches!(parse(src), Err(Error::Unsupported(m)) if m.contains("`times`")));
}

#[test]
fn shared_gates_cannot_blow_up() {
    // Each level uses the next twice: 2^18 nodes when expanded, more than allowed but few
    // enough to build quickly if the limit were broken.
    let mut src = String::new();
    for i in 0..18 {
        src += &format!(
            "Gate: G{i}\n- type: AND\n- inputs: G{n}, G{n}\n\n",
            n = i + 1
        );
    }
    src += "Event: G18\n";
    let e = parse_err(&src);
    assert!(e.contains(&MAX_NODES.to_string()), "{e}");
}

#[test]
fn deep_chains_are_refused_not_overflowed() {
    let n = 2000;
    let mut src = String::new();
    for i in 0..n {
        src += &format!("Gate: G{i}\n- inputs: G{}\n\n", i + 1);
    }
    src += &format!("Event: G{n}\n");
    // The parser recurses, so give it the stack the binaries have.
    let e = std::thread::Builder::new()
        .stack_size(8 << 20)
        .spawn(move || parse_err(&src))
        .unwrap()
        .join()
        .unwrap();
    assert!(e.contains("levels deep"), "{e}");
}

#[test]
fn same_id_over_different_subtrees_gets_its_own_paragraph() {
    let (mut t, db) = parse(
        "Gate: T\n- type: AND\n- inputs: M, N\n\nGate: M\n- type: OR\n- inputs: A, B\n\nGate: N\n- type: OR\n- inputs: A, C\n\nEvent: A\n\nEvent: B\n\nEvent: C\n",
    )
    .unwrap();
    // Give the second intermediate event the first one's id, over a different subtree.
    t.root.children[0].children[1].id = "M".into();
    let text = write_text(&t, &db).unwrap();
    assert!(text.contains("Gate: M_1\n- id: M\n"), "{text}");
    assert_eq!(parse(&text).unwrap().0, t);
}

#[test]
fn intermediate_event_named_like_an_event() {
    let (mut t, db) = parse(SMALL).unwrap();
    t.root.id = "A".into();
    let text = write_text(&t, &db).unwrap();
    assert!(text.contains("Gate: A_1\n- id: A\n"), "{text}");
    assert_eq!(parse(&text).unwrap().0, t);
}

#[test]
fn writer_refuses_what_it_cannot_hold() {
    let (mut t, db) = parse(SMALL).unwrap();
    // A gate directly under a gate.
    let g = t.root.children[0].clone();
    t.root.children[0].children.push(g);
    assert!(write_text(&t, &db).is_err());
    let (mut t, db) = parse(SMALL).unwrap();
    t.root.children[0].children[0].id = "has space".into();
    assert!(write_text(&t, &db).is_err());
}
