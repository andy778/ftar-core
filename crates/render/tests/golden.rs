//! Golden scenes: the complete drawing (every path, text and hit box) of the example trees and
//! of one tree with every symbol kind, compared with files in `tests/golden/`. Any change to
//! the layout or the symbols shows up here; when the change is intended, regenerate with
//! `FTAR_UPDATE_GOLDEN=1 cargo test -p ftar-render --test golden` and review the diff.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use ftar_core::edit::new_node;
use ftar_core::project::Project;
use ftar_core::{EventType, PrimaryEventDb, Tree};
use ftar_render::{tree_scene, SceneOptions};

const DATA: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../core/tests/data");

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

/// Compare `actual` with `tests/golden/<name>.txt`, or write it when FTAR_UPDATE_GOLDEN is set.
fn check(name: &str, actual: &str) {
    let path = golden_dir().join(format!("{name}.txt"));
    if std::env::var_os("FTAR_UPDATE_GOLDEN").is_some() {
        std::fs::create_dir_all(golden_dir()).unwrap();
        std::fs::write(&path, actual).unwrap();
        return;
    }
    let want = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "{}: {e}; create it with FTAR_UPDATE_GOLDEN=1",
            path.display()
        )
    });
    // git may check the file out with CRLF line ends
    let want = want.replace("\r\n", "\n");
    if want != actual {
        let line = want
            .lines()
            .zip(actual.lines())
            .position(|(w, a)| w != a)
            .map_or_else(|| "the end".to_string(), |i| format!("line {}", i + 1));
        panic!(
            "{name}: scene differs from {} at {line}; if intended, regenerate with \
             FTAR_UPDATE_GOLDEN=1 and review the diff",
            path.display()
        );
    }
}

fn scene_text(p: &Project, title: &str, probabilities: bool) -> String {
    let opts = SceneOptions {
        title: title.into(),
        probabilities,
        ..SceneOptions::default()
    };
    let s = tree_scene(&p.tree, &p.db, &opts);
    // one primitive or hit box per line, so a change diffs to the lines it touches
    let mut out = format!("size {} x {}\n", s.width, s.height);
    for prim in &s.prims {
        out += &format!("{prim:?}\n");
    }
    for it in &s.items {
        out += &format!("item {:?} {:?} {:?}\n", it.path, it.bbox, it.prims);
    }
    out
}

#[test]
fn example_trees_with_header_and_probabilities() {
    for name in [
        "theatre",
        "top",
        "middle",
        "bottom",
        "wrap/wrap",
        "transtest/transTest",
    ] {
        let p = Project::load(format!("{DATA}/{name}.fta")).unwrap();
        let title = name.rsplit('/').next().unwrap();
        check(&name.replace('/', "_"), &scene_text(&p, title, true));
    }
}

/// A one-event tree under a long title: the header, not the drawing, sets the width.
#[test]
fn header_wider_than_the_tree_sets_the_width() {
    let mut tree = Tree::default();
    tree.root.children[0]
        .children
        .push(new_node(EventType::Basic, "E"));
    let p = Project {
        tree,
        db: PrimaryEventDb::default(),
    };
    check(
        "wide_header",
        &scene_text(
            &p,
            "A title far wider than the small tree drawn under it",
            false,
        ),
    );
}

#[test]
fn plain_drawing_has_no_header_or_probabilities() {
    let p = Project::load(format!("{DATA}/theatre.fta")).unwrap();
    check("theatre_plain", &scene_text(&p, "", false));
}

/// Every symbol kind, a dormant event (ring behind it), an inhibit gate with a condition that
/// is not analysed (dashed, beside the gate), a transfer-out, long descriptions and a note.
#[test]
fn every_symbol_kind() {
    use EventType::*;
    let mut tree = Tree {
        note: "A note long enough to be wrapped onto more than one line of the header".into(),
        db_name: Some("every.ped".into()),
        ..Tree::default()
    };
    let gate = &mut tree.root.children[0];
    for (i, k) in [
        Basic,
        Undeveloped,
        External,
        Dormant,
        TransferIn,
        Intermediate,
        And,
        Or,
        Xor,
        PriorityAnd,
        Unknown,
    ]
    .into_iter()
    .enumerate()
    {
        let mut n = new_node(k, &format!("N{i}"));
        n.text = format!("Description of event number {i}, which wraps inside its symbol");
        gate.children.push(n);
    }
    let mut dormant = new_node(Basic, "DORM");
    dormant.modifier = Some('D');
    gate.children.push(dormant);
    let mut inhibit = new_node(Inhibit, "H");
    inhibit.children.push(new_node(ConditionNotAnalysed, "C"));
    inhibit.children.push(new_node(Basic, "B"));
    gate.children.push(inhibit);
    let mut with_out = new_node(Or, "G");
    with_out.children.push(new_node(TransferOut, "OUT"));
    with_out.children.push(new_node(ConditionAnalysed, "CA"));
    gate.children.push(with_out);

    let p = Project {
        tree,
        db: PrimaryEventDb::default(),
    };
    check("every_kind", &scene_text(&p, "every kind", true));
}
