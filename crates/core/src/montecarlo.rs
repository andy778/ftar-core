//! Monte Carlo simulation of the top event, after OpenFTA's
//! `generate_monte_carlo_simulation`.
//!
//! The default [`Method::FirstFailure`] is OpenFTA's "fast" method: only runs
//! with at least one component failure are simulated. The *first* failed event
//! (first in list order, not in time) is drawn from its exact conditional
//! distribution and every later event fails independently with its own
//! probability. The estimate is then `P(any failure) * failures / tests`, which
//! is unbiased and far less noisy than drawing every event when failures are
//! rare. [`Method::Direct`] is OpenFTA's `SLOW` variant: every event is drawn on
//! every trial.
//!
//! The system fails when every event of some minimal cut set has failed. Every
//! distinct failure vector is recorded with its count (OpenFTA keeps them all:
//! `MONTE_MAX_F` is 0 = unlimited). The PRNG is built in, so results are
//! reproducible per seed (they cannot match the C `rand()` stream).

use std::collections::HashMap;

use crate::analysis::{minimal_cut_sets, CutSet};
use crate::error::{Error, Result};
use crate::model::{EventType, PrimaryEventDb, Tree};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Method {
    /// OpenFTA's default: simulate only runs with at least one failure.
    #[default]
    FirstFailure,
    /// Draw every event on every trial (OpenFTA's `SLOW`).
    Direct,
}

#[derive(Debug, Clone)]
pub struct MonteCarloOptions {
    pub trials: u64,
    pub unit_time: f64,
    pub seed: u64,
    pub method: Method,
}

impl Default for MonteCarloOptions {
    fn default() -> Self {
        Self {
            trials: 100_000,
            unit_time: 1.0,
            seed: 0x9E37_79B9_7F4A_7C15,
            method: Method::FirstFailure,
        }
    }
}

#[derive(Debug, Clone)]
pub struct MonteCarloReport {
    pub method: Method,
    pub trials: u64,
    /// Simulated runs in which the top event occurred.
    pub system_failures: u64,
    /// Estimated probability of the top event: `bas_prob * failures / trials`.
    pub probability: f64,
    /// One standard error, as OpenFTA prints it: `probability / sqrt(failures)`.
    pub std_error: f64,
    /// Exact probability that at least one component fails.
    pub any_component_failure: f64,
    /// Factor applied to raw counts: `any_component_failure` for the
    /// first-failure method, `1.0` for the direct method.
    pub bas_prob: f64,
    /// For each primary event: number of *simulated* system failures in which it had failed.
    pub event_failures: Vec<(String, u64)>,
    /// In how many simulated runs each minimal cut set was fully failed.
    pub cut_set_hits: Vec<(CutSet, u64)>,
    /// Every distinct failure vector that caused a system failure, with its
    /// count, most frequent first (ties in lexicographic order).
    pub modes: Vec<(CutSet, u64)>,
}

struct Rng(u64);
impl Rng {
    /// Scramble the seed (splitmix64) so that neighbouring seeds give unrelated
    /// streams; the xorshift state must be non-zero.
    fn seeded(seed: u64) -> Self {
        let mut z = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        Rng(if z == 0 { 0x9E37_79B9_7F4A_7C15 } else { z })
    }
    fn next_f64(&mut self) -> f64 {
        // xorshift64*
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        (x.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11) as f64 / (1u64 << 53) as f64
    }
}

pub fn simulate(
    tree: &Tree,
    db: &PrimaryEventDb,
    opts: &MonteCarloOptions,
) -> Result<MonteCarloReport> {
    let sets = minimal_cut_sets(tree)?;
    let mut ids: Vec<String> = Vec::new();
    for n in tree.nodes() {
        if (n.kind.is_primary() || n.kind == EventType::ConditionAnalysed) && !ids.contains(&n.id) {
            ids.push(n.id.clone());
        }
    }
    let probs: Vec<f64> = ids
        .iter()
        .map(|id| {
            db.effective_probability(id, opts.unit_time)
                .ok_or_else(|| Error::MissingEvent(id.clone()))
        })
        .collect::<Result<_>>()?;
    let indexed: Vec<Vec<usize>> = sets
        .iter()
        .map(|s| {
            s.iter()
                .filter_map(|id| ids.iter().position(|x| x == id))
                .collect()
        })
        .collect();

    // calc_bas_parms: P(at least one failure) and P(event i is the first failure | any failure)
    let mut p0 = 1.0f64;
    let mut rel: Vec<f64> = Vec::with_capacity(probs.len());
    for p in &probs {
        rel.push(p0 * p);
        p0 *= 1.0 - p;
    }
    let any = 1.0 - p0;
    if any > 0.0 {
        for r in &mut rel {
            *r /= any;
        }
    }
    let first_failure = opts.method == Method::FirstFailure;
    let bas_prob = if first_failure { any } else { 1.0 };

    let mut rng = Rng::seeded(opts.seed);
    let mut failed = vec![false; ids.len()];
    let mut event_failures = vec![0u64; ids.len()];
    let mut hits = vec![0u64; sets.len()];
    let mut modes: HashMap<Vec<usize>, u64> = HashMap::new();
    let mut system_failures = 0u64;

    // All primary event probabilities zero: nothing can fail (OpenFTA abandons the analysis).
    let runnable = any > 0.0;
    for _ in 0..if runnable { opts.trials } else { 0 } {
        failed.fill(false);
        let mut start = 0;
        if first_failure {
            // rand_disc: pick the first failure from `rel`
            let u = rng.next_f64();
            let mut acc = 0.0;
            let mut pick = rel.len() - 1;
            for (i, r) in rel.iter().enumerate() {
                acc += r;
                if u < acc {
                    pick = i;
                    break;
                }
            }
            failed[pick] = true;
            start = pick + 1;
        }
        for j in start..failed.len() {
            failed[j] = rng.next_f64() < probs[j];
        }
        let mut top = false;
        for (h, cs) in hits.iter_mut().zip(&indexed) {
            if cs.iter().all(|&e| failed[e]) {
                *h += 1;
                top = true;
            }
        }
        if top {
            system_failures += 1;
            for (c, f) in event_failures.iter_mut().zip(&failed) {
                *c += u64::from(*f);
            }
            let vec: Vec<usize> = (0..failed.len()).filter(|&i| failed[i]).collect();
            *modes.entry(vec).or_insert(0) += 1;
        }
    }

    let n = opts.trials.max(1) as f64;
    let probability = bas_prob * system_failures as f64 / n;
    let std_error = if system_failures > 0 {
        probability / (system_failures as f64).sqrt()
    } else {
        0.0
    };
    let mut modes: Vec<(CutSet, u64)> = modes
        .into_iter()
        .map(|(v, c)| (v.into_iter().map(|i| ids[i].clone()).collect(), c))
        .collect();
    modes.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    Ok(MonteCarloReport {
        method: opts.method,
        trials: opts.trials,
        system_failures,
        probability,
        std_error,
        any_component_failure: any,
        bas_prob,
        event_failures: ids.into_iter().zip(event_failures).collect(),
        cut_set_hits: sets.into_iter().zip(hits).collect(),
        modes,
    })
}
