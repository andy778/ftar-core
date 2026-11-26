//! Golden scenes: the complete drawing (every path, text and hit box) of the example trees and
//! of one tree with every symbol kind, compared with files in `tests/golden/`. Any change to
//! the layout or the symbols shows up here; when the change is intended, regenerate with
//! `FTAR_UPDATE_GOLDEN=1 cargo test -p ftar-render --test golden` and review the diff.
//! Coordinates may differ by rounding ([`TOLERANCE`]); everything else must match exactly.
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
    if let Some(place) = difference(&want, actual) {
        panic!(
            "{name}: scene differs from {} at {place}; if intended, regenerate with \
             FTAR_UPDATE_GOLDEN=1 and review the diff",
            path.display()
        );
    }
}

/// How far apart two coordinates may be. The arcs come from `sin` and `cos`, whose last bits
/// differ between platforms (seen on macOS).
const TOLERANCE: f64 = 0.001;

/// Where `actual` first differs from `want`, if it does.
fn difference(want: &str, actual: &str) -> Option<String> {
    let line = want
        .lines()
        .zip(actual.lines())
        .position(|(w, a)| !same_line(w, a));
    match line {
        Some(i) => Some(format!("line {}", i + 1)),
        None if want.lines().count() != actual.lines().count() => Some("the end".into()),
        None => None,
    }
}

/// Equal text, with the numbers outside quoted strings equal within [`TOLERANCE`].
fn same_line(want: &str, actual: &str) -> bool {
    let (want_text, want_numbers) = split_numbers(want);
    let (text, numbers) = split_numbers(actual);
    want_text == text
        && want_numbers.len() == numbers.len()
        && want_numbers
            .iter()
            .zip(&numbers)
            .all(|(w, a)| (w - a).abs() <= TOLERANCE)
}

/// The line with each number outside a quoted string replaced by `#`, and those numbers.
fn split_numbers(line: &str) -> (String, Vec<f64>) {
    let mut text = String::new();
    let mut numbers = Vec::new();
    let (mut quoted, mut escaped) = (false, false);
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        let minus = c == '-' && chars.peek().is_some_and(char::is_ascii_digit);
        if !quoted && (c.is_ascii_digit() || minus) {
            let mut number = String::from(c);
            while let Some(d) = chars.next_if(|d| d.is_ascii_digit() || ".e-".contains(*d)) {
                number.push(d);
            }
            match number.parse() {
                Ok(n) => {
                    numbers.push(n);
                    text.push('#');
                }
                Err(_) => text += &number,
            }
        } else {
            quoted ^= c == '"' && !escaped;
            escaped = quoted && c == '\\' && !escaped;
            text.push(c);
        }
    }
    (text, numbers)
}

#[test]
fn comparison_allows_only_rounding_differences() {
    let want = "pts: [[125.57224, -4.371139e-8]], text: \"p = 0.0001\"\nsize 2\n";
    let rounded = "pts: [[125.57225, 4.371139e-8]], text: \"p = 0.0001\"\nsize 2\n";
    assert_eq!(difference(want, rounded), None);
    for (from, to, place) in [
        ("125.57224", "125.57424", "line 1"), // a moved point
        ("0.0001", "0.0002", "line 1"),       // text is exact
        ("pts", "box", "line 1"),
        ("]],", ", 1.0]],", "line 1"), // an extra number
        ("size 2\n", "size 2\nsize 2\n", "the end"),
    ] {
        let changed = want.replace(from, to);
        assert_eq!(difference(want, &changed).as_deref(), Some(place), "{to}");
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
