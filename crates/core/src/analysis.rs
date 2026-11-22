//! Minimal cut sets, exact top-event probability and importance measures.
//!
//! Semantics follow OpenFTA's algebraic method:
//! * AND / inhibit / priority-AND → product of children
//! * OR / XOR → union of children (XOR is analysed as OR, as in OpenFTA)
//! * an *analysed* condition as first child of an OR/XOR is ANDed with the result
//! * *not analysed* conditions and transfer-outs are ignored

use std::collections::BTreeSet;

use crate::error::{Error, Result};
use crate::model::{EventType, Node, PrimaryEventDb, Tree};

/// A cut set: a set of primary event ids that together cause the top event.
pub type CutSet = BTreeSet<String>;

/// Minimal cut sets of every order.
pub fn minimal_cut_sets(tree: &Tree) -> Result<Vec<CutSet>> {
    minimal_cut_sets_limited(tree, usize::MAX)
}

/// Minimal cut sets of order ≤ `max_order`, sorted by order then name.
pub fn minimal_cut_sets_limited(tree: &Tree, max_order: usize) -> Result<Vec<CutSet>> {
    Ok(eval(&tree.root, max_order)?
        .map(minimise)
        .unwrap_or_default())
}

fn eval(n: &Node, limit: usize) -> Result<Option<Vec<CutSet>>> {
    use EventType::*;
    match n.kind {
        Basic | Undeveloped | External | Dormant | ConditionAnalysed => {
            if n.id.is_empty() {
                return Err(Error::Parse("primary event without an id".into()));
            }
            Ok((limit >= 1)
                .then(|| vec![CutSet::from([n.id.clone()])])
                .or(Some(vec![])))
        }
        ConditionNotAnalysed | TransferOut => Ok(None),
        TransferIn => Err(Error::Unsupported(format!(
            "transfer-in `{}` is not inlined; analyse the tree with its transfers loaded",
            n.id
        ))),
        Unknown => Err(Error::Parse("event of unknown type".into())),
        Or | Xor | Intermediate => {
            let mut kids = n.children.iter().peekable();
            let mut cond = None;
            if kids.peek().is_some_and(|c| c.kind == ConditionAnalysed) {
                cond = eval(kids.next().unwrap(), limit)?;
            }
            if kids.peek().is_some_and(|c| c.kind == ConditionNotAnalysed) {
                kids.next();
            }
            let mut acc: Option<Vec<CutSet>> = None;
            for c in kids {
                if let Some(s) = eval(c, limit)? {
                    acc.get_or_insert_with(Vec::new).extend(s);
                }
            }
            let acc = acc.map(minimise);
            Ok(match (acc, cond) {
                (Some(a), Some(c)) => Some(and(&a, &c, limit)),
                (None, c) => c,
                (a, None) => a,
            })
        }
        And | Inhibit | PriorityAnd => {
            let mut acc: Option<Vec<CutSet>> = None;
            for c in &n.children {
                if let Some(s) = eval(c, limit)? {
                    acc = Some(match acc {
                        None => s,
                        Some(a) => and(&a, &s, limit),
                    });
                }
            }
            Ok(acc)
        }
    }
}

fn and(a: &[CutSet], b: &[CutSet], limit: usize) -> Vec<CutSet> {
    let mut out = Vec::with_capacity(a.len() * b.len());
    for x in a {
        for y in b {
            let u: CutSet = x.union(y).cloned().collect();
            if u.len() <= limit {
                out.push(u);
            }
        }
    }
    minimise(out)
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

// ---------------------------------------------------------------- probability

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TopProbability {
    /// Sum of cut-set probabilities (upper bound, accurate for small p).
    pub rare_event: f64,
    /// Min-cut upper bound: 1 - ∏(1 - P(cutset)).
    pub min_cut_upper_bound: f64,
}

pub fn cut_set_probability(cs: &CutSet, db: &PrimaryEventDb, unit_time: f64) -> Result<f64> {
    cs.iter().try_fold(1.0, |p, id| {
        db.effective_probability(id, unit_time)
            .map(|q| p * q)
            .ok_or_else(|| Error::MissingEvent(id.clone()))
    })
}

/// Fast bounds. For the exact value use [`probability_report`].
pub fn top_probability(cut_sets: &[CutSet], db: &PrimaryEventDb) -> Result<TopProbability> {
    let mut sum = 0.0;
    let mut survive = 1.0;
    for cs in cut_sets {
        let p = cut_set_probability(cs, db, 1.0)?;
        sum += p;
        survive *= 1.0 - p;
    }
    Ok(TopProbability {
        rare_event: sum.min(1.0),
        min_cut_upper_bound: 1.0 - survive,
    })
}

#[derive(Debug, Clone)]
pub struct ProbabilityOptions {
    /// Use only cut sets up to this order.
    pub max_order: usize,
    /// Number of terms of the inclusion–exclusion expansion (clamped to the number of cut sets).
    pub terms: usize,
    /// Unit time for failure-rate (`L`) events.
    pub unit_time: f64,
}

/// Default number of inclusion-exclusion terms: enough for the exact value on
/// trees with up to this many cut sets (cost grows as 2^n).
pub const DEFAULT_TERMS: usize = 16;

impl Default for ProbabilityOptions {
    fn default() -> Self {
        Self {
            max_order: usize::MAX,
            terms: DEFAULT_TERMS,
            unit_time: 1.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct EventImportance {
    pub id: String,
    /// Sum of the probabilities of the cut sets containing the event.
    pub contribution: f64,
    /// `100 * contribution / top`.
    pub importance_pct: f64,
}

#[derive(Debug, Clone)]
pub struct ProbabilityReport {
    pub cut_sets: Vec<(CutSet, f64)>,
    /// Signed inclusion–exclusion terms: `+Σ P(Ci)`, `-Σ P(Ci∧Cj)`, …
    pub terms: Vec<f64>,
    /// Running sums; alternately upper and lower bounds.
    pub partial_sums: Vec<f64>,
    pub top: f64,
    /// True when all terms were used, so `top` is exact.
    pub exact: bool,
    pub events: Vec<EventImportance>,
}

pub fn probability_report(
    tree: &Tree,
    db: &PrimaryEventDb,
    opts: &ProbabilityOptions,
) -> Result<ProbabilityReport> {
    let sets = minimal_cut_sets_limited(tree, opts.max_order)?;

    // Primary events in OpenFTA list order, then index them.
    let ids = tree.primary_event_ids();
    let probs: Vec<f64> = ids
        .iter()
        .map(|id| {
            db.effective_probability(id, opts.unit_time)
                .ok_or_else(|| Error::MissingEvent(id.clone()))
        })
        .collect::<Result<_>>()?;
    let idx = |id: &String| ids.iter().position(|x| x == id).unwrap();
    let indexed: Vec<Vec<usize>> = sets.iter().map(|s| s.iter().map(idx).collect()).collect();

    let cut_sets: Vec<(CutSet, f64)> = sets
        .iter()
        .zip(&indexed)
        .map(|(s, ix)| (s.clone(), ix.iter().map(|&i| probs[i]).product()))
        .collect();

    let n_terms = opts.terms.min(sets.len());
    let mut terms = Vec::new();
    let mut partial_sums = Vec::new();
    let mut top = 0.0;
    let mut counts = vec![0u32; ids.len()];
    for k in 1..=n_terms {
        let sum = combos(&indexed, &probs, &mut counts, 0, k, 1.0);
        let term = if k % 2 == 1 { sum } else { -sum };
        top += term;
        terms.push(term);
        partial_sums.push(top);
    }

    let events = ids
        .iter()
        .enumerate()
        .map(|(i, id)| {
            let contribution: f64 = indexed
                .iter()
                .zip(&cut_sets)
                .filter(|(ix, _)| ix.contains(&i))
                .map(|(_, (_, p))| p)
                .sum::<f64>()
                + 0.0; // avoid -0.0 for events in no cut set
            EventImportance {
                id: id.clone(),
                contribution,
                importance_pct: if top > 0.0 {
                    100.0 * contribution / top
                } else {
                    0.0
                },
            }
        })
        .collect();

    Ok(ProbabilityReport {
        exact: n_terms == sets.len(),
        cut_sets,
        terms,
        partial_sums,
        top,
        events,
    })
}

/// Σ over all `k`-combinations (from index `start`) of P(union of the chosen cut sets).
fn combos(
    sets: &[Vec<usize>],
    probs: &[f64],
    counts: &mut [u32],
    start: usize,
    k: usize,
    prod: f64,
) -> f64 {
    if k == 0 {
        return prod;
    }
    let mut total = 0.0;
    for i in start..=sets.len().saturating_sub(k) {
        let mut p = prod;
        for &e in &sets[i] {
            if counts[e] == 0 {
                p *= probs[e];
            }
            counts[e] += 1;
        }
        total += combos(sets, probs, counts, i + 1, k - 1, p);
        for &e in &sets[i] {
            counts[e] -= 1;
        }
    }
    total
}
