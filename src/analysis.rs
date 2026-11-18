//! Minimal cut sets and top-event probability.

use std::collections::BTreeSet;

use crate::error::{Error, Result};
use crate::model::{EventType, Node, PrimaryEventDb, Tree};

/// A cut set: a set of primary event ids that together cause the top event.
pub type CutSet = BTreeSet<String>;

/// Compute the minimal cut sets of a tree (MOCUS-style expansion + absorption).
///
/// Sets are returned sorted by order (size), then lexicographically.
///
/// Current limits (ported incrementally from OpenFTA): transfer gates and
/// XOR gates return [`Error::Unsupported`]; priority-AND and inhibit gates are
/// analysed as AND; "not analysed" conditions are ignored.
pub fn minimal_cut_sets(tree: &Tree) -> Result<Vec<CutSet>> {
    let sets = expand(&tree.root)?;
    Ok(minimise(sets))
}

fn expand(n: &Node) -> Result<Vec<CutSet>> {
    use EventType::*;
    match n.kind {
        Basic | Undeveloped | External | Dormant | ConditionAnalysed => {
            if n.id.is_empty() {
                return Err(Error::Parse("primary event without an id".into()));
            }
            Ok(vec![CutSet::from([n.id.clone()])])
        }
        Intermediate | Or => {
            let mut out = Vec::new();
            for c in n.children.iter().filter(|c| c.kind != ConditionNotAnalysed) {
                out.extend(expand(c)?);
            }
            Ok(minimise(out))
        }
        And | PriorityAnd | Inhibit => {
            let mut acc: Vec<CutSet> = vec![CutSet::new()];
            for c in n.children.iter().filter(|c| c.kind != ConditionNotAnalysed) {
                let rhs = expand(c)?;
                let mut next = Vec::with_capacity(acc.len() * rhs.len());
                for a in &acc {
                    for b in &rhs {
                        next.push(a.union(b).cloned().collect());
                    }
                }
                acc = minimise(next);
            }
            Ok(acc)
        }
        Xor => Err(Error::Unsupported("XOR gates (non-coherent logic)".into())),
        TransferIn | TransferOut => Err(Error::Unsupported("transfer gates (multi-file trees)".into())),
        ConditionNotAnalysed => Ok(vec![CutSet::new()]),
        Unknown => Err(Error::Parse("event of unknown type".into())),
    }
}

/// Remove duplicates and supersets (absorption law), sort by order.
fn minimise(mut sets: Vec<CutSet>) -> Vec<CutSet> {
    sets.sort_by(|a, b| a.len().cmp(&b.len()).then_with(|| a.cmp(b)));
    sets.dedup();
    let mut out: Vec<CutSet> = Vec::new();
    for s in sets {
        if !out.iter().any(|k| k.is_subset(&s)) {
            out.push(s);
        }
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TopProbability {
    /// Sum of cut-set probabilities (upper bound, accurate for small p).
    pub rare_event: f64,
    /// Min-cut upper bound: 1 - ∏(1 - P(cutset)).
    pub min_cut_upper_bound: f64,
}

pub fn cut_set_probability(cs: &CutSet, db: &PrimaryEventDb) -> Result<f64> {
    cs.iter().try_fold(1.0, |p, id| {
        db.probability(id).map(|q| p * q).ok_or_else(|| Error::MissingEvent(id.clone()))
    })
}

pub fn top_probability(cut_sets: &[CutSet], db: &PrimaryEventDb) -> Result<TopProbability> {
    let mut sum = 0.0;
    let mut survive = 1.0;
    for cs in cut_sets {
        let p = cut_set_probability(cs, db)?;
        sum += p;
        survive *= 1.0 - p;
    }
    Ok(TopProbability { rare_event: sum.min(1.0), min_cut_upper_bound: 1.0 - survive })
}
