//! Large cut set results must not change when the engine's representation does.
//!
//! `old_cut_sets` is the original `BTreeSet<String>` algorithm, kept here
//! verbatim as a reference. The production engine (interned `u32` ids, sorted
//! boxed slices) must return exactly the same cut sets in the same order, also
//! when the order limit drops sets or conditions are involved.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeSet;

use ftar_core::analysis::{minimal_cut_sets, minimal_cut_sets_limited, CutSet};
use ftar_core::model::{EventType, Node, Tree};

type Old = BTreeSet<String>;

fn old_cut_sets(tree: &Tree, limit: usize) -> Vec<Old> {
    old_eval(&tree.root, limit)
        .map(old_minimise)
        .unwrap_or_default()
}

fn old_eval(n: &Node, limit: usize) -> Option<Vec<Old>> {
    use EventType::*;
    match n.kind {
        Basic | Undeveloped | External | Dormant | ConditionAnalysed => (limit >= 1)
            .then(|| vec![Old::from([n.id.clone()])])
            .or(Some(vec![])),
        ConditionNotAnalysed | TransferOut => None,
        TransferIn | Unknown => unreachable!("not generated"),
        Or | Xor | Intermediate => {
            let mut kids = n.children.iter().peekable();
            let mut cond = None;
            if kids.peek().is_some_and(|c| c.kind == ConditionAnalysed) {
                cond = old_eval(kids.next().unwrap(), limit);
            }
            if kids.peek().is_some_and(|c| c.kind == ConditionNotAnalysed) {
                kids.next();
            }
            let mut acc: Option<Vec<Old>> = None;
            for c in kids {
                if let Some(s) = old_eval(c, limit) {
                    acc.get_or_insert_with(Vec::new).extend(s);
                }
            }
            let acc = acc.map(old_minimise);
            match (acc, cond) {
                (Some(a), Some(c)) => Some(old_and(&a, &c, limit)),
                (None, c) => c,
                (a, None) => a,
            }
        }
        And | Inhibit | PriorityAnd => {
            let mut acc: Option<Vec<Old>> = None;
            for c in &n.children {
                if let Some(s) = old_eval(c, limit) {
                    acc = Some(match acc {
                        None => s,
                        Some(a) => old_and(&a, &s, limit),
                    });
                }
            }
            acc
        }
    }
}

fn old_and(a: &[Old], b: &[Old], limit: usize) -> Vec<Old> {
    let mut out = Vec::with_capacity(a.len() * b.len());
    for x in a {
        for y in b {
            let u: Old = x.union(y).cloned().collect();
            if u.len() <= limit {
                out.push(u);
            }
        }
    }
    old_minimise(out)
}

fn old_minimise(mut sets: Vec<Old>) -> Vec<Old> {
    sets.sort_by(|a, b| a.len().cmp(&b.len()).then_with(|| a.cmp(b)));
    sets.dedup();
    let mut out: Vec<Old> = Vec::new();
    for s in sets {
        if !out.iter().any(|k| k.is_subset(&s)) {
            out.push(s);
        }
    }
    out
}

fn node(kind: EventType, id: &str, children: Vec<Node>) -> Node {
    Node {
        kind,
        modifier: None,
        id: id.into(),
        text: String::new(),
        children,
    }
}

fn basic(id: &str) -> Node {
    node(EventType::Basic, id, vec![])
}

fn tree(root: Node) -> Tree {
    Tree {
        db_name: None,
        note: String::new(),
        root,
    }
}

fn assert_same(t: &Tree, limit: usize) {
    let old = old_cut_sets(t, limit);
    let new = minimal_cut_sets_limited(t, limit).unwrap();
    assert_eq!(old.len(), new.len(), "cut set count (limit {limit})");
    for (o, n) in old.iter().zip(&new) {
        let o: Vec<&str> = o.iter().map(String::as_str).collect();
        let n: Vec<&str> = n.iter().collect();
        assert_eq!(o, n, "limit {limit}");
    }
}

/// AND of `k` two-event ORs: 2^k cut sets of order k.
fn andor(k: usize) -> Tree {
    let ors = (0..k)
        .map(|i| {
            node(
                EventType::Or,
                "",
                vec![basic(&format!("A{i:02}")), basic(&format!("B{i:02}"))],
            )
        })
        .collect();
    tree(node(EventType::And, "", ors))
}

/// OR of `k` three-event ANDs.
fn orand(k: usize) -> Tree {
    let ands = (0..k)
        .map(|i| {
            node(
                EventType::And,
                "",
                (0..3).map(|j| basic(&format!("E{i:03}_{j}"))).collect(),
            )
        })
        .collect();
    tree(node(EventType::Or, "", ands))
}

/// Shared events, nesting, absorption (supersets must disappear) and an
/// analysed condition, built deterministically.
fn mixed(seed: u64) -> Tree {
    let mut s = seed;
    let mut next = move |n: usize| {
        s = s
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((s >> 33) as usize) % n
    };
    fn build(depth: usize, next: &mut dyn FnMut(usize) -> usize) -> Node {
        if depth == 0 || next(4) == 0 {
            return basic(&format!("X{:02}", next(14)));
        }
        let kind = if next(2) == 0 {
            EventType::And
        } else {
            EventType::Or
        };
        let n = 2 + next(3);
        let mut kids: Vec<Node> = (0..n).map(|_| build(depth - 1, next)).collect();
        if kind == EventType::Or && next(3) == 0 {
            kids.insert(0, node(EventType::ConditionAnalysed, "COND", vec![]));
        }
        node(kind, "", kids)
    }
    tree(build(5, &mut next))
}

#[test]
fn andor_family_matches_original_algorithm() {
    for k in [1, 2, 5, 9] {
        let t = andor(k);
        assert_same(&t, usize::MAX);
        let cs = minimal_cut_sets(&t).unwrap();
        assert_eq!(cs.len(), 1 << k);
        assert!(cs.iter().all(|c| c.len() == k));
    }
    assert_same(&andor(8), 7); // limit drops every set
    assert_same(&andor(8), 8);
}

#[test]
fn orand_family_matches_original_algorithm() {
    assert_same(&orand(300), usize::MAX);
    assert_same(&orand(50), 2);
    assert_same(&orand(50), 0);
}

#[test]
fn mixed_trees_match_original_algorithm() {
    for seed in 0..200 {
        let t = mixed(seed);
        for limit in [usize::MAX, 4, 2, 1] {
            assert_same(&t, limit);
        }
    }
}

#[test]
fn results_are_sorted_by_order_then_name() {
    let cs = minimal_cut_sets(&mixed(7)).unwrap();
    assert!(cs
        .windows(2)
        .all(|w| (w[0].len(), &w[0]) < (w[1].len(), &w[1])));
    let _: &[CutSet] = &cs;
}
