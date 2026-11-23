//! Minimal cut sets, exact top-event probability and importance measures.
//!
//! Semantics follow OpenFTA's algebraic method:
//! * AND / inhibit / priority-AND → product of children
//! * OR / XOR → union of children (XOR is analysed as OR, as in OpenFTA)
//! * an *analysed* condition as first child of an OR/XOR is ANDed with the result
//! * *not analysed* conditions and transfer-outs are ignored

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use crate::error::{Error, Result};
use crate::model::{EventType, Node, PrimaryEventDb, Tree};

/// A cut set: a set of primary event ids that together cause the top event.
///
/// Stored as one boxed slice of shared event names, sorted by name and free of
/// duplicates, so it iterates in the same order as the `BTreeSet<String>` it
/// replaced and orders (`Ord`) the same way. Each event name is allocated once
/// per analysis and shared by every cut set that contains it, so a cut set
/// costs one pointer pair per event instead of a `String` plus tree nodes.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct CutSet(Box<[Arc<str>]>);

impl CutSet {
    /// Build from any events; sorts and removes duplicates.
    pub fn new<I, S>(events: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<Arc<str>>,
    {
        let mut v: Vec<Arc<str>> = events.into_iter().map(Into::into).collect();
        v.sort();
        v.dedup();
        CutSet(v.into_boxed_slice())
    }

    /// Number of events (the order of the cut set).
    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Events in name order.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = &str> + DoubleEndedIterator + Clone {
        self.0.iter().map(|e| &**e)
    }

    /// True when every event of `self` is also in `other`.
    pub fn is_subset(&self, other: &CutSet) -> bool {
        let mut rest = other.0.iter();
        self.0.iter().all(|e| rest.any(|o| o == e))
    }

    /// Event names joined by `sep`.
    pub fn join(&self, sep: &str) -> String {
        self.0.join(sep)
    }

    pub fn contains(&self, id: &str) -> bool {
        self.0.binary_search_by(|e| (**e).cmp(id)).is_ok()
    }
}

impl<S: Into<Arc<str>>> FromIterator<S> for CutSet {
    fn from_iter<I: IntoIterator<Item = S>>(iter: I) -> Self {
        CutSet::new(iter)
    }
}

impl<S: Into<Arc<str>>, const N: usize> From<[S; N]> for CutSet {
    fn from(a: [S; N]) -> Self {
        CutSet::new(a)
    }
}

/// Minimal cut sets of every order.
pub fn minimal_cut_sets(tree: &Tree) -> Result<Vec<CutSet>> {
    minimal_cut_sets_limited(tree, usize::MAX)
}

/// Minimal cut sets of order ≤ `max_order`, sorted by order then name.
pub fn minimal_cut_sets_limited(tree: &Tree, max_order: usize) -> Result<Vec<CutSet>> {
    // Intern every event name once, in name order, so comparing the `u32` ids
    // is the same as comparing the names.
    let mut names = BTreeSet::new();
    collect_ids(&tree.root, &mut names);
    let table: Vec<Arc<str>> = names.into_iter().map(Arc::from).collect();
    let cx = Interner { table };
    let sets = match eval(&tree.root, max_order, &cx)? {
        Some(v) => minimise(v),
        None => Vec::new(),
    };
    Ok(sets
        .into_iter()
        .map(|s| CutSet(s.iter().map(|&i| cx.table[i as usize].clone()).collect()))
        .collect())
}

struct Interner {
    table: Vec<Arc<str>>,
}

impl Interner {
    fn id(&self, name: &str) -> Option<u32> {
        self.table
            .binary_search_by(|e| (**e).cmp(name))
            .ok()
            .and_then(|i| u32::try_from(i).ok())
    }
}

fn collect_ids<'a>(n: &'a Node, out: &mut BTreeSet<&'a str>) {
    use EventType::*;
    if matches!(
        n.kind,
        Basic | Undeveloped | External | Dormant | ConditionAnalysed
    ) && !n.id.is_empty()
    {
        out.insert(&n.id);
    }
    for c in &n.children {
        collect_ids(c, out);
    }
}

/// A cut set during computation: interned event ids, sorted ascending.
type Ids = Box<[u32]>;

fn eval(n: &Node, limit: usize, cx: &Interner) -> Result<Option<Vec<Ids>>> {
    use EventType::*;
    match n.kind {
        Basic | Undeveloped | External | Dormant | ConditionAnalysed => {
            if n.id.is_empty() {
                return Err(Error::Parse("primary event without an id".into()));
            }
            let id = cx
                .id(&n.id)
                .ok_or_else(|| Error::Parse("primary event id was not interned".into()))?;
            Ok((limit >= 1).then(|| vec![Box::from([id])]).or(Some(vec![])))
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
                cond = match kids.next() {
                    Some(c) => eval(c, limit, cx)?,
                    None => None,
                };
            }
            if kids.peek().is_some_and(|c| c.kind == ConditionNotAnalysed) {
                kids.next();
            }
            let mut acc: Option<Vec<Ids>> = None;
            for c in kids {
                if let Some(s) = eval(c, limit, cx)? {
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
            let mut acc: Option<Vec<Ids>> = None;
            for c in &n.children {
                if let Some(s) = eval(c, limit, cx)? {
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

/// Sorted union of two sorted id slices into `buf` (cleared first).
fn union_into(x: &[u32], y: &[u32], buf: &mut Vec<u32>) {
    use std::cmp::Ordering::*;
    buf.clear();
    let (mut i, mut j) = (0, 0);
    while i < x.len() && j < y.len() {
        match x[i].cmp(&y[j]) {
            Less => {
                buf.push(x[i]);
                i += 1;
            }
            Greater => {
                buf.push(y[j]);
                j += 1;
            }
            Equal => {
                buf.push(x[i]);
                i += 1;
                j += 1;
            }
        }
    }
    buf.extend_from_slice(&x[i..]);
    buf.extend_from_slice(&y[j..]);
}

/// Is sorted `k` a subset of sorted `s`?
fn is_subset(k: &[u32], s: &[u32]) -> bool {
    let mut j = 0;
    for &e in k {
        while j < s.len() && s[j] < e {
            j += 1;
        }
        if j == s.len() || s[j] != e {
            return false;
        }
        j += 1;
    }
    true
}

fn and(a: &[Ids], b: &[Ids], limit: usize) -> Vec<Ids> {
    let mut out: Vec<Ids> = Vec::new();
    let mut buf = Vec::new();
    for x in a {
        for y in b {
            union_into(x, y, &mut buf);
            if buf.len() <= limit {
                out.push(buf.as_slice().into());
            }
        }
    }
    minimise(out)
}

/// Remove duplicates and supersets (absorption law), sort by order.
fn minimise(mut sets: Vec<Ids>) -> Vec<Ids> {
    sets.sort_by(|a, b| a.len().cmp(&b.len()).then_with(|| a.cmp(b)));
    sets.dedup();
    // Sorted by length and duplicate-free, so a kept set can only absorb
    // strictly longer sets: compare against the kept sets shorter than `s`.
    let mut out: Vec<Ids> = Vec::with_capacity(sets.len());
    let mut shorter = 0;
    let mut cur_len = 0;
    for s in sets {
        if s.len() != cur_len {
            cur_len = s.len();
            shorter = out.len();
        }
        if !out[..shorter].iter().any(|k| is_subset(k, &s)) {
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
            .ok_or_else(|| Error::MissingEvent(id.to_string()))
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
    let pos: HashMap<&str, usize> = ids
        .iter()
        .enumerate()
        .map(|(i, id)| (id.as_str(), i))
        .collect();
    let indexed: Vec<Vec<usize>> = sets
        .iter()
        .map(|s| s.iter().filter_map(|e| pos.get(e).copied()).collect())
        .collect();

    let probabilities: Vec<f64> = indexed
        .iter()
        .map(|ix| ix.iter().map(|&i| probs[i]).product())
        .collect();
    let n_sets = sets.len();
    let cut_sets: Vec<(CutSet, f64)> = sets.into_iter().zip(probabilities).collect();

    let n_terms = opts.terms.min(n_sets);
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
        exact: n_terms == n_sets,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cut_set_is_sorted_deduplicated_and_shares_names() {
        let a: Arc<str> = Arc::from("B");
        let cs = CutSet::new([a.clone(), Arc::from("A"), Arc::from("B"), Arc::from("C")]);
        assert_eq!(cs.len(), 3);
        assert!(!cs.is_empty());
        assert_eq!(cs.iter().collect::<Vec<_>>(), ["A", "B", "C"]);
        assert_eq!(cs.iter().next_back(), Some("C"));
        assert!(cs.contains("B") && !cs.contains("D") && !cs.contains(""));
        assert_eq!(cs.join(" "), "A B C");
        // Cloning shares the event names instead of copying them.
        let c2 = cs.clone();
        assert!(Arc::ptr_eq(&c2.0[1], &cs.0[1]));
        assert!(Arc::strong_count(&a) >= 3); // `a`, the set, the clone
    }

    #[test]
    fn cut_set_orders_and_compares_like_a_btree_set() {
        let names: [&[&str]; 5] = [&[], &["A"], &["A", "B"], &["A", "C"], &["B"]];
        for x in names {
            for y in names {
                let (bx, by): (BTreeSet<&str>, BTreeSet<&str>) =
                    (x.iter().copied().collect(), y.iter().copied().collect());
                let (cx, cy): (CutSet, CutSet) =
                    (x.iter().copied().collect(), y.iter().copied().collect());
                assert_eq!(bx.cmp(&by), cx.cmp(&cy), "{x:?} vs {y:?}");
                assert_eq!(bx.is_subset(&by), cx.is_subset(&cy), "{x:?} <= {y:?}");
            }
        }
        assert_eq!(CutSet::from(["b", "a"]), CutSet::from(["a", "b", "a"]));
        assert!(CutSet::default().is_empty());
    }

    #[test]
    fn id_set_helpers() {
        let mut buf = Vec::new();
        union_into(&[1, 3, 5], &[2, 3, 6, 7], &mut buf);
        assert_eq!(buf, [1, 2, 3, 5, 6, 7]);
        assert!(is_subset(&[3, 5], &[1, 3, 5, 9]));
        assert!(!is_subset(&[3, 4], &[1, 3, 5, 9]));
        assert!(is_subset(&[], &[1]));
        let sets: Vec<Ids> = vec![
            Box::from([1, 2, 3]),
            Box::from([1, 2]),
            Box::from([1, 2]),
            Box::from([4]),
        ];
        let m = minimise(sets);
        assert_eq!(m, vec![Box::from([4u32]) as Ids, Box::from([1u32, 2])]);
    }
}
