//! The OpenFTA-style drawing shared by the SVG export and the GUI canvas (#52).

use ftar_core::edit::new_node;
use ftar_core::project::{Document, Project};
use ftar_core::{EventType, PrimaryEventDb, Tree};
use ftar_render::scene::{format_g, triangulate, Prim, UNIT};
use ftar_render::{tree_scene, Scene, SceneOptions};

const DATA: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../core/tests/data");

fn load(name: &str) -> Project {
    Project::load(format!("{DATA}/{name}")).unwrap()
}

fn texts(s: &Scene) -> Vec<&str> {
    s.prims
        .iter()
        .filter_map(|p| match p {
            Prim::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

fn item<'a>(s: &'a Scene, path: &[usize]) -> &'a ftar_render::scene::Item {
    s.items.iter().find(|i| i.path == path).expect("item")
}

fn centre_x(b: [f32; 4]) -> f32 {
    (b[0] + b[2]) / 2.0
}

#[test]
fn theatre_has_openfta_labels_and_header() {
    let p = load("theatre.fta");
    let s = tree_scene(
        &p.tree,
        &p.db,
        &SceneOptions {
            title: "theatre.fta".into(),
            probabilities: true,
        },
    );
    let t = texts(&s);
    for want in [
        "Tree: theatre.fta",
        "Database: theatre.ped",
        "Top",
        "AND1",
        "Or1",
        "MAINS_FAIL",
        "0.03",
        "0.05",
        "0.02",
        "Mains Power",
        "Fails.",
    ] {
        assert!(t.contains(&want), "{want} missing from {t:?}");
    }
    // the tree's note is in the header
    assert!(t.iter().any(|l| l.starts_with("Failure Of An Electrical")));
    // every node is an item, and the scene contains all of them
    assert_eq!(s.items.len(), p.tree.nodes().len());
    for it in &s.items {
        let b = it.bbox;
        assert!(b[0] >= 0.0 && b[1] >= 0.0 && b[2] <= s.width && b[3] <= s.height);
    }
}

#[test]
fn layout_follows_openfta_spacing() {
    let p = load("theatre.fta");
    let s = tree_scene(&p.tree, &p.db, &SceneOptions::default());
    // Top -> AND1 -> [Backup_Fail -> Or1 -> [RELAY_FAIL, GEN_FAIL], MAINS_FAIL]
    let top = item(&s, &[]);
    let gate = item(&s, &[0]);
    let relay = item(&s, &[0, 0, 0, 0]);
    let gen = item(&s, &[0, 0, 0, 1]);
    // one unit between rows (symbols hang from their top centre)
    assert!((gate.bbox[1] - top.bbox[1] - UNIT).abs() < 0.01);
    // 1.4 units between columns
    assert!((centre_x(gen.bbox) - centre_x(relay.bbox) - 1.4 * UNIT).abs() < 0.01);
    // parents are centred over their children
    let or = item(&s, &[0, 0, 0]);
    assert!((centre_x(or.bbox) - (centre_x(relay.bbox) + centre_x(gen.bbox)) / 2.0).abs() < 0.01);
}

#[test]
fn transfer_out_sits_beside_its_parent() {
    // as edited and drawn: transfers not expanded
    let p = Document::load(format!("{DATA}/threemotor/T.fta")).unwrap();
    assert_eq!(p.tree.root.children[0].kind, EventType::TransferOut);
    let s = tree_scene(&p.tree, &p.db, &SceneOptions::default());
    let e1 = item(&s, &[]);
    let t = item(&s, &[0]);
    let or1 = item(&s, &[1]);
    assert!(t.bbox[0] > e1.bbox[2], "right of the parent");
    assert!(t.bbox[1] < e1.bbox[3], "on the parent's row");
    assert!(or1.bbox[1] > e1.bbox[3], "the gate is under the parent");
    assert!((centre_x(or1.bbox) - centre_x(e1.bbox)).abs() < 0.01);
}

#[test]
fn every_kind_draws_and_concave_gates_fill() {
    use EventType::*;
    let mut tree = Tree::default();
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
        Inhibit,
        Unknown,
    ]
    .into_iter()
    .enumerate()
    {
        tree.root.children[0]
            .children
            .push(new_node(k, &format!("N{i}")));
    }
    let mut inhibit = new_node(Inhibit, "H");
    inhibit.children.push(new_node(ConditionNotAnalysed, "C"));
    inhibit.children.push(new_node(Basic, "B"));
    tree.root.children[0].children.push(inhibit);
    let s = tree_scene(&tree, &PrimaryEventDb::default(), &SceneOptions::default());
    for it in &s.items {
        assert!(!it.prims.is_empty(), "{:?} draws something", it.path);
    }
    // every filled outline triangulates into a full fan of triangles
    for prim in &s.prims {
        if let Prim::Path {
            pts,
            closed: true,
            fill: Some(_),
            ..
        } = prim
        {
            assert_eq!(triangulate(pts).len(), pts.len() - 2, "{pts:?}");
        }
    }
    // the condition hangs beside the inhibit gate, dashed (not analysed)
    let h = s.items.iter().find(|i| i.path.ends_with(&[12])).unwrap();
    let mut c_path = h.path.clone();
    c_path.push(0);
    let c = item(&s, &c_path);
    assert!(c.bbox[0] > h.bbox[2]);
    assert!(s.prims[c.prims.clone()]
        .iter()
        .any(|p| matches!(p, Prim::Path { dashed: true, .. })));
}

#[test]
fn hit_testing_finds_the_symbol_under_a_point() {
    let p = load("theatre.fta");
    let s = tree_scene(&p.tree, &p.db, &SceneOptions::default());
    let mains = item(&s, &[0, 1]);
    let b = mains.bbox;
    let hit = s.item_at([centre_x(b), (b[1] + b[3]) / 2.0]).unwrap();
    assert_eq!(hit.path, vec![0, 1]);
    assert!(s.item_at([1.0, 1.0]).is_none());
}

#[test]
fn concave_or_outline_triangulates_inside_itself() {
    // an arch with a curved notch in its base, like the OR gate
    let pts = [[0.0, 0.0], [2.0, 0.0], [2.0, 2.0], [1.0, 1.0], [0.0, 2.0]];
    let tris = triangulate(&pts);
    assert_eq!(tris.len(), 3);
    let area: f32 = tris
        .iter()
        .map(|t| {
            let (a, b, c) = (pts[t[0]], pts[t[1]], pts[t[2]]);
            ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])).abs() / 2.0
        })
        .sum();
    assert!((area - 3.0).abs() < 1e-5, "{area}");
    assert!(triangulate(&pts[..2]).is_empty());
}

#[test]
fn probabilities_print_like_c_percent_g() {
    for (v, want) in [
        (0.03, "0.03"),
        (0.5, "0.5"),
        (1.0, "1"),
        (0.0, "0"),
        (1.5e-7, "1.5e-07"),
        (2.5e-5, "2.5e-05"),
        (1.23456789e-3, "0.00123457"),
        (1234567.0, "1.23457e+06"),
    ] {
        assert_eq!(format_g(v), want, "{v}");
    }
}
