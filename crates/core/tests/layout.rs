#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! `ftar_core::layout`: leaves take consecutive columns left to right, each parent sits midway
//! between its first and last child, rows are depths, and `parent`/`path` lead back to the node.

use ftar_core::layout::{layout, Placed};
use ftar_core::parse::parse_fta;
use ftar_core::{EventType, Node};

fn node(kind: EventType, id: &str, children: Vec<Node>) -> Node {
    Node {
        kind,
        modifier: None,
        id: id.into(),
        text: String::new(),
        children,
    }
}

fn leaf(id: &str) -> Node {
    node(EventType::Basic, id, vec![])
}

fn ids<'a>(placed: &[Placed<'a>]) -> Vec<&'a str> {
    placed.iter().map(|p| p.node.id.as_str()).collect()
}

#[test]
fn a_single_event_sits_at_the_origin() {
    let root = leaf("E1");
    let placed = layout(&root);
    assert_eq!(placed.len(), 1);
    let p = &placed[0];
    assert!(std::ptr::eq(p.node, &root));
    assert_eq!((p.x, p.y), (0.0, 0.0));
    assert_eq!(p.parent, None);
    assert!(p.path.is_empty());
}

/// ```text
///              TOP (1.125, 0)
///             /              ///         A (0, 1)        G (2.25, 1)
///                       /     |      ///                  B (1, 2) E (2, 2)  C (3.5, 2)
///                                    /         ///                               D (3, 3)     F (4, 3)
/// ```
/// Worked by hand: leaves A, B, E, D, F take columns 0..=4 in pre-order. C is midway between
/// D and F (3.5); G is midway between its first and last child B and C (2.25), which is not the
/// mean of all three (2.17); TOP is midway between A and G.
#[test]
fn positions_of_a_small_tree_worked_by_hand() {
    use EventType::*;
    let root = node(
        Or,
        "TOP",
        vec![
            leaf("A"),
            node(
                And,
                "G",
                vec![
                    leaf("B"),
                    leaf("E"),
                    node(Or, "C", vec![leaf("D"), leaf("F")]),
                ],
            ),
        ],
    );
    let placed = layout(&root);

    // Pre-order: a parent comes before its children, children in file order.
    assert_eq!(ids(&placed), ["TOP", "A", "G", "B", "E", "C", "D", "F"]);
    let at = |id: &str| placed.iter().find(|p| p.node.id == id).unwrap();
    let pos = |id: &str| (at(id).x, at(id).y);
    assert_eq!(pos("A"), (0.0, 1.0));
    assert_eq!(pos("B"), (1.0, 2.0));
    assert_eq!(pos("E"), (2.0, 2.0));
    assert_eq!(pos("D"), (3.0, 3.0));
    assert_eq!(pos("F"), (4.0, 3.0));
    assert_eq!(pos("C"), (3.5, 2.0));
    assert_eq!(pos("G"), (2.25, 1.0));
    assert_eq!(pos("TOP"), (1.125, 0.0));

    let parent = |id: &str| at(id).parent.map(|i| placed[i].node.id.as_str());
    assert_eq!(parent("TOP"), None);
    assert_eq!(parent("A"), Some("TOP"));
    assert_eq!(parent("G"), Some("TOP"));
    assert_eq!(parent("C"), Some("G"));
    assert_eq!(parent("F"), Some("C"));

    assert_eq!(at("TOP").path, Vec::<usize>::new());
    assert_eq!(at("G").path, [1]);
    assert_eq!(at("C").path, [1, 2]);
    assert_eq!(at("F").path, [1, 2, 1]);
}

/// The same rules on every example tree that ships with OpenFTA.
#[test]
fn layout_rules_hold_on_the_example_trees() {
    for name in [
        "theatre.fta",
        "lift.fta",
        "top.fta",
        "middle.fta",
        "bottom.fta",
    ] {
        let bytes =
            std::fs::read(format!("{}/tests/data/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap();
        let tree = parse_fta(&bytes).unwrap();
        let placed = layout(&tree.root);
        assert_eq!(
            placed.len(),
            tree.nodes().len(),
            "{name}: every node placed once"
        );
        assert!(
            std::ptr::eq(placed[0].node, &tree.root),
            "{name}: root first"
        );

        let mut next_leaf_column = 0.0;
        for (i, p) in placed.iter().enumerate() {
            // `path` walks from the root to this very node.
            let mut walked = &tree.root;
            for &k in &p.path {
                walked = &walked.children[k];
            }
            assert!(std::ptr::eq(walked, p.node), "{name}: path of node {i}");
            assert_eq!(
                p.y,
                p.path.len() as f32,
                "{name}: row of node {i} is its depth"
            );

            match p.parent {
                None => assert_eq!(i, 0, "{name}: only the root has no parent"),
                Some(up) => {
                    assert!(up < i, "{name}: parent placed before child {i}");
                    assert_eq!(
                        placed[up].path,
                        p.path[..p.path.len() - 1],
                        "{name}: parent of {i}"
                    );
                }
            }

            let kids: Vec<&Placed> = placed.iter().filter(|c| c.parent == Some(i)).collect();
            assert_eq!(kids.len(), p.node.children.len(), "{name}: children of {i}");
            if kids.is_empty() {
                assert_eq!(
                    p.x, next_leaf_column,
                    "{name}: leaves fill columns in order"
                );
                next_leaf_column += 1.0;
            } else {
                let mid = (kids[0].x + kids[kids.len() - 1].x) / 2.0;
                assert_eq!(p.x, mid, "{name}: node {i} centred over its children");
            }
        }
    }
}
