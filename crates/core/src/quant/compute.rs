//! Probability and intensity of cut sets and of their disjunctions, at one
//! time and sample, after the formulas PFTA publishes (Vesely's kinetic tree
//! theory):
//!
//! * cut set `C`: `q[C] = ∏ q[e]`, `ω[C] = Σ ω[e] q[C ÷ e]`;
//! * gate `T = C1 + … + CN`: `q[T]` by inclusion-exclusion, and
//!   `ω[T] = ω¹[T] − ω²[T]` with
//!   `ω¹ = Σ ω[Ci] − Σ ω[gcd(Ci,Cj)] q[CiCj ÷ gcd] + …`,
//!   `ω² = Σ ω†[Ci] − Σ ω†[Ci,Cj] + …`,
//!   `ω†[Ci,…] = Σa ω[gcd ÷ Ca] q[Ca (CiCj… ÷ gcd)] − Σa<b … + …`.
//!
//! Both expansions stop at a given order, or once a term is below a share
//! (the tolerance) of the sum. A budget on the number of combinations keeps
//! a large tree from running for ever.

use crate::error::{Error, Result};

/// A cut set: sorted, distinct event indices.
pub(crate) type Term = Vec<u32>;

/// Quantities of the events at one time and sample.
pub(crate) struct Point<'a> {
    pub q: &'a [f64],
    pub w: &'a [f64],
}

impl Point<'_> {
    fn q(&self, e: u32) -> f64 {
        self.q.get(e as usize).copied().unwrap_or(f64::NAN)
    }
    fn w(&self, e: u32) -> f64 {
        self.w.get(e as usize).copied().unwrap_or(f64::NAN)
    }

    /// Product of the probabilities of `events`, largest first (as PFTA,
    /// so the result does not depend on the order of the events).
    fn q_of(&self, events: impl Iterator<Item = u32>) -> f64 {
        let mut f: Vec<f64> = events.map(|e| self.q(e)).collect();
        f.sort_by(|a, b| b.total_cmp(a));
        f.into_iter().product()
    }

    /// `ω[G] = Σ_{e∈G} ω[e] q[G ÷ e]`, terms summed largest first.
    fn w_of(&self, g: &[u32]) -> f64 {
        let mut terms: Vec<f64> = g
            .iter()
            .map(|&e| self.w(e) * self.q_of(g.iter().copied().filter(|&f| f != e)))
            .collect();
        terms.sort_by(|a, b| b.total_cmp(a));
        // `+ 0.0`: an empty sum of floats is -0.0 in Rust.
        terms.into_iter().sum::<f64>() + 0.0
    }

    pub(crate) fn term_q(&self, t: &[u32]) -> f64 {
        self.q_of(t.iter().copied())
    }

    pub(crate) fn term_w(&self, t: &[u32]) -> f64 {
        self.w_of(t)
    }
}

/// Settings of the expansions.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Truncation {
    pub order: Option<usize>,
    pub tolerance: f64,
}

/// How many combinations may still be visited.
pub(crate) struct Budget(pub u64);

impl Budget {
    fn spend(&mut self) -> Result<()> {
        if self.0 == 0 {
            return Err(Error::Unsupported(
                "the expansion needs too many terms: set `computational_order` (1 is the rare-event approximation)"
                    .into(),
            ));
        }
        self.0 -= 1;
        Ok(())
    }
}

fn done(r: usize, latest: f64, sum: f64, t: Truncation) -> bool {
    t.order == Some(r) || latest.is_nan() || latest == 0.0 || latest.abs() < sum.abs() * t.tolerance
}

fn sign(order: usize) -> f64 {
    if order % 2 == 1 {
        1.0
    } else {
        -1.0
    }
}

/// Visit every `k`-combination of `terms`, giving the chosen indices.
fn each_combination(
    n: usize,
    k: usize,
    budget: &mut Budget,
    f: &mut dyn FnMut(&[usize]),
) -> Result<()> {
    if k > n {
        return Ok(());
    }
    let mut c: Vec<usize> = (0..k).collect();
    loop {
        budget.spend()?;
        f(&c);
        let Some(i) = (0..k)
            .rev()
            .find(|&i| c.get(i).is_some_and(|&x| x < n - k + i))
        else {
            return Ok(());
        };
        if let Some(x) = c.get_mut(i) {
            *x += 1;
        }
        for j in i + 1..k {
            let prev = c.get(j - 1).copied().unwrap_or(0);
            if let Some(x) = c.get_mut(j) {
                *x = prev + 1;
            }
        }
    }
}

fn union(terms: &[Term], pick: &[usize]) -> Term {
    let mut u: Term = pick
        .iter()
        .filter_map(|&i| terms.get(i))
        .flatten()
        .copied()
        .collect();
    u.sort_unstable();
    u.dedup();
    u
}

fn gcd(terms: &[Term], pick: &[usize]) -> Term {
    let mut sets = pick.iter().filter_map(|&i| terms.get(i));
    let Some(first) = sets.next() else {
        return Term::new();
    };
    let rest: Vec<&Term> = sets.collect();
    first
        .iter()
        .copied()
        .filter(|e| rest.iter().all(|t| t.binary_search(e).is_ok()))
        .collect()
}

fn minus(a: &[u32], b: &[u32]) -> Term {
    a.iter()
        .copied()
        .filter(|e| b.binary_search(e).is_err())
        .collect()
}

/// `q[T]` by inclusion-exclusion.
pub(crate) fn probability(
    terms: &[Term],
    p: &Point,
    t: Truncation,
    budget: &mut Budget,
) -> Result<f64> {
    let mut sum = 0.0;
    for r in 1..=terms.len() {
        let mut s = 0.0;
        each_combination(terms.len(), r, budget, &mut |pick| {
            s += p.term_q(&union(terms, pick));
        })?;
        let latest = sign(r) * s;
        sum += latest;
        if done(r, latest, sum, t) {
            break;
        }
    }
    Ok(sum)
}

/// Contribution of order `r` to `ω¹`.
fn omega_1(terms: &[Term], p: &Point, r: usize, budget: &mut Budget) -> Result<f64> {
    let mut s = 0.0;
    each_combination(terms.len(), r, budget, &mut |pick| {
        let g = gcd(terms, pick);
        if !g.is_empty() {
            s += p.w_of(&g) * p.term_q(&minus(&union(terms, pick), &g));
        }
    })?;
    Ok(sign(r) * s)
}

/// `ω†` of the combination `pick`, contribution of order `o`.
fn omega_dagger(
    terms: &[Term],
    p: &Point,
    pick: &[usize],
    o: usize,
    budget: &mut Budget,
) -> Result<f64> {
    let g = gcd(terms, pick);
    if g.is_empty() {
        return Ok(0.0);
    }
    let rest = minus(&union(terms, pick), &g);
    let mut s = 0.0;
    each_combination(terms.len(), o, budget, &mut |failed| {
        let uf = union(terms, failed);
        let h = minus(&g, &uf);
        if !h.is_empty() {
            let mut all = uf;
            all.extend_from_slice(&rest);
            all.sort_unstable();
            all.dedup();
            s += p.w_of(&h) * p.term_q(&all);
        }
    })?;
    Ok(sign(o) * s)
}

/// Contribution of order `s` to `ω²`, with the `ω†` orders `orders`.
fn omega_2(
    terms: &[Term],
    p: &Point,
    s: usize,
    orders: std::ops::Range<usize>,
    budget: &mut Budget,
) -> Result<f64> {
    if s == 0 {
        return Ok(0.0);
    }
    let mut picks = Vec::new();
    each_combination(terms.len(), s, budget, &mut |pick| {
        picks.push(pick.to_vec())
    })?;
    let mut total = 0.0;
    for pick in &picks {
        for o in orders.clone() {
            total += omega_dagger(terms, p, pick, o, budget)?;
        }
    }
    Ok(sign(s) * total)
}

/// `ω[T] = ω¹ − ω²`, expanded order by order: the contribution of order `r`
/// is `ω¹_r − Σ_{s<r−1} ω²_s(†_{r−1}) − ω²_{r−1}(†_{1..r−1})`.
pub(crate) fn intensity(
    terms: &[Term],
    p: &Point,
    t: Truncation,
    budget: &mut Budget,
) -> Result<f64> {
    let mut sum = 0.0;
    for r in 1..=terms.len() {
        let mut latest = omega_1(terms, p, r, budget)?;
        for s in 1..r.saturating_sub(1) {
            latest -= omega_2(terms, p, s, r - 1..r, budget)?;
        }
        latest -= omega_2(terms, p, r - 1, 1..r, budget)?;
        sum += latest;
        if done(r, latest, sum, t) {
            break;
        }
    }
    Ok(sum)
}

/// `x / y`, where a zero `y` gives `x · ∞` (as PFTA's `robust_divide`).
pub(crate) fn divide(x: f64, y: f64) -> f64 {
    if y == 0.0 {
        x * f64::INFINITY
    } else {
        x / y
    }
}

/// Drop sets that contain a smaller one, keep the rest in order of size.
pub(crate) fn minimise(mut sets: Vec<Term>) -> Vec<Term> {
    sets.sort_by(|a, b| a.len().cmp(&b.len()).then_with(|| a.cmp(b)));
    sets.dedup();
    let mut out: Vec<Term> = Vec::new();
    for s in sets {
        if !out
            .iter()
            .any(|k| k.iter().all(|e| s.binary_search(e).is_ok()))
        {
            out.push(s);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: Truncation = Truncation {
        order: None,
        tolerance: 0.0,
    };

    #[test]
    fn toast_example() {
        // BF (ω 0.1) OR (TF (ω 0.2) AND TB (q 0.75) AND BSD (q 0.9)).
        let q = [0.0, 0.0, 0.75, 0.9];
        let w = [0.1, 0.2, 0.0, 0.0];
        let p = Point { q: &q, w: &w };
        let terms = vec![vec![0], vec![1, 2, 3]];
        let mut b = Budget(1000);
        assert_eq!(probability(&terms, &p, ALL, &mut b).unwrap(), 0.0);
        let w = intensity(&terms, &p, ALL, &mut b).unwrap();
        assert!((w - (0.1 + 0.2 * 0.75 * 0.9)).abs() < 1e-15, "{w}");
    }

    #[test]
    fn shared_event_intensity_is_exact() {
        // T = AB + AC, A ω=a with q; exact ω = Σ ω_e (Q(e=1) - Q(e=0)).
        let q = [0.1, 0.2, 0.3];
        let w = [0.5, 0.7, 0.11];
        let p = Point { q: &q, w: &w };
        let terms = vec![vec![0, 1], vec![0, 2]];
        let mut b = Budget(1000);
        let qt = probability(&terms, &p, ALL, &mut b).unwrap();
        let exact_q = 0.1 * (1.0 - 0.8 * 0.7);
        assert!((qt - exact_q).abs() < 1e-15);
        let birnbaum = [1.0 - 0.8 * 0.7, 0.1 * 0.7, 0.1 * 0.8];
        let exact_w: f64 = birnbaum.iter().zip(w).map(|(b, w)| b * w).sum();
        let wt = intensity(&terms, &p, ALL, &mut b).unwrap();
        assert!((wt - exact_w).abs() < 1e-15, "{wt} vs {exact_w}");
    }

    #[test]
    fn order_one_is_the_rare_event_approximation() {
        let q = [0.1, 0.2];
        let w = [0.0, 0.0];
        let p = Point { q: &q, w: &w };
        let t = Truncation {
            order: Some(1),
            tolerance: 0.0,
        };
        let terms = vec![vec![0], vec![1]];
        assert!((probability(&terms, &p, t, &mut Budget(10)).unwrap() - 0.3).abs() < 1e-15);
        assert!(probability(&terms, &p, ALL, &mut Budget(0)).is_err());
    }

    #[test]
    fn minimise_absorbs() {
        let m = minimise(vec![vec![1, 2], vec![1], vec![2, 3], vec![1, 2]]);
        assert_eq!(m, vec![vec![1], vec![2, 3]]);
        assert_eq!(divide(1.0, 0.0), f64::INFINITY);
        assert!(divide(0.0, 0.0).is_nan());
    }
}
