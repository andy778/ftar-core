//! Importance of the events of a gate `T`, at each time and sample, as PFTA
//! defines them (`Q` is a probability, `T[e=1]` the gate with event `e`
//! always failed, `T[e=0]` never failed):
//!
//! * marginal (Birnbaum): `Q(T[e=1]) − Q(T[e=0])`
//! * criticality: marginal · `Q(e) / Q(T)`
//! * diagnostic (Fussell–Vesely): `Q(cut sets with e) / Q(T)`
//! * prognostic: `(Q(T) − Q(T[e=0])) / Q(T)`
//! * risk achievement worth: `Q(T[e=1]) / Q(T)`
//! * risk reduction worth: `Q(T) / Q(T[e=0])`
//!
//! Every `Q` of a gate uses the same expansion and truncation as the gate
//! itself; divisions by zero give `x · ∞` as in PFTA.

use super::compute::{self, Budget, Point, Term, Truncation};
use super::EventResult;
use crate::error::Result;

/// The importance measures of one event in one gate, at every point.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Importance {
    pub event: String,
    pub label: Option<String>,
    pub marginal: Vec<f64>,
    pub criticality: Vec<f64>,
    pub diagnostic: Vec<f64>,
    pub prognostic: Vec<f64>,
    pub risk_achievement_worth: Vec<f64>,
    pub risk_reduction_worth: Vec<f64>,
}

/// The gate's cut sets with event `e` failed: `e` removed from each, then
/// the sets that contain others dropped.
fn with_true(terms: &[Term], e: u32) -> Vec<Term> {
    compute::minimise(
        terms
            .iter()
            .map(|t| t.iter().copied().filter(|&x| x != e).collect())
            .collect(),
    )
}

/// The gate's cut sets that do (`keep`) or do not contain `e`.
fn containing(terms: &[Term], e: u32, keep: bool) -> Vec<Term> {
    terms
        .iter()
        .filter(|t| t.binary_search(&e).is_ok() == keep)
        .cloned()
        .collect()
}

/// `q` and `ω` of every event at point `i`.
pub(crate) fn point_values(events: &[EventResult], i: usize) -> (Vec<f64>, Vec<f64>) {
    let at = |v: &[f64]| v.get(i).copied().unwrap_or(f64::NAN);
    (
        events.iter().map(|e| at(&e.series.q)).collect(),
        events.iter().map(|e| at(&e.series.w)).collect(),
    )
}

/// The importance of every event in the gate's cut sets, in event order.
pub(crate) fn importances(
    terms: &[Term],
    events: &[EventResult],
    gate_q: &[f64],
    trunc: Truncation,
    budget: &mut Budget,
) -> Result<Vec<Importance>> {
    let mut implicated: Vec<u32> = terms.iter().flatten().copied().collect();
    implicated.sort_unstable();
    implicated.dedup();
    let mut out = Vec::with_capacity(implicated.len());
    for e in implicated {
        let Some(ev) = events.get(e as usize) else {
            continue;
        };
        let (on, off, with) = (
            with_true(terms, e),
            containing(terms, e, false),
            containing(terms, e, true),
        );
        let mut m = Importance {
            event: ev.id.clone(),
            label: ev.label.clone(),
            ..Importance::default()
        };
        for (i, &qg) in gate_q.iter().enumerate() {
            let (q, w) = point_values(events, i);
            let p = Point { q: &q, w: &w };
            let q_on = compute::probability(&on, &p, trunc, budget)?;
            let q_off = compute::probability(&off, &p, trunc, budget)?;
            let q_with = compute::probability(&with, &p, trunc, budget)?;
            let q_e = ev.series.q.get(i).copied().unwrap_or(f64::NAN);
            let marginal = q_on - q_off;
            m.marginal.push(marginal);
            m.criticality.push(marginal * compute::divide(q_e, qg));
            m.diagnostic.push(compute::divide(q_with, qg));
            m.prognostic.push(compute::divide(qg - q_off, qg));
            m.risk_achievement_worth.push(compute::divide(q_on, qg));
            m.risk_reduction_worth.push(compute::divide(qg, q_off));
        }
        out.push(m);
    }
    Ok(out)
}
