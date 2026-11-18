//! Monte Carlo simulation of the top event.
//!
//! Each trial draws every primary event as failed/not failed from its
//! probability; the system fails when every event of some minimal cut set
//! has failed. Uses a small built-in PRNG so results are reproducible per seed.

use crate::analysis::{minimal_cut_sets, CutSet};
use crate::error::{Error, Result};
use crate::model::{EventType, PrimaryEventDb, Tree};

#[derive(Debug, Clone)]
pub struct MonteCarloOptions {
    pub trials: u64,
    pub unit_time: f64,
    pub seed: u64,
}

impl Default for MonteCarloOptions {
    fn default() -> Self {
        Self { trials: 100_000, unit_time: 1.0, seed: 0x9E37_79B9_7F4A_7C15 }
    }
}

#[derive(Debug, Clone)]
pub struct MonteCarloReport {
    pub trials: u64,
    pub system_failures: u64,
    /// Estimated probability of the top event.
    pub probability: f64,
    /// One standard error of the estimate.
    pub std_error: f64,
    /// Exact probability that at least one component fails.
    pub any_component_failure: f64,
    /// For each primary event: number of system failures in which it had failed.
    pub event_failures: Vec<(String, u64)>,
    /// How often each minimal cut set was fully failed.
    pub cut_set_hits: Vec<(CutSet, u64)>,
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

pub fn simulate(tree: &Tree, db: &PrimaryEventDb, opts: &MonteCarloOptions) -> Result<MonteCarloReport> {
    let sets = minimal_cut_sets(tree)?;
    let mut ids: Vec<String> = Vec::new();
    for n in tree.nodes() {
        if (n.kind.is_primary() || n.kind == EventType::ConditionAnalysed) && !ids.contains(&n.id) {
            ids.push(n.id.clone());
        }
    }
    let probs: Vec<f64> = ids
        .iter()
        .map(|id| db.effective_probability(id, opts.unit_time).ok_or_else(|| Error::MissingEvent(id.clone())))
        .collect::<Result<_>>()?;
    let indexed: Vec<Vec<usize>> = sets
        .iter()
        .map(|s| s.iter().map(|id| ids.iter().position(|x| x == id).unwrap()).collect())
        .collect();

    let mut rng = Rng::seeded(opts.seed);
    let mut failed = vec![false; ids.len()];
    let mut event_failures = vec![0u64; ids.len()];
    let mut hits = vec![0u64; sets.len()];
    let mut system_failures = 0u64;

    for _ in 0..opts.trials {
        for (f, p) in failed.iter_mut().zip(&probs) {
            *f = rng.next_f64() < *p;
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
                *c += *f as u64;
            }
        }
    }

    let n = opts.trials.max(1) as f64;
    let p = system_failures as f64 / n;
    Ok(MonteCarloReport {
        trials: opts.trials,
        system_failures,
        probability: p,
        std_error: (p * (1.0 - p) / n).sqrt(),
        any_component_failure: 1.0 - probs.iter().map(|p| 1.0 - p).product::<f64>(),
        event_failures: ids.into_iter().zip(event_failures).collect(),
        cut_set_hits: sets.into_iter().zip(hits).collect(),
    })
}
