//! Time-dependent quantification with uncertainty, as PFTA does it: the
//! probability `q(t)`, intensity `ω(t)` and rate `λ(t) = ω/(1-q)` of every
//! event, cut set and gate, at each of the file's `times` and for each
//! sample of the parameter distributions.
//!
//! Events follow their model (`Fixed`, `ConstantRate`, `True`, `False`).
//! Each gate's minimal cut sets come from ftar's cut-set engine (so the
//! OpenFTA gate types work too), with `True` events removed from them and
//! sets with a `False` event dropped. Gate quantities use the expansions in
//! [`compute`], truncated by `computational_order` and
//! `computational_tolerance`.
//!
//! Sampling follows PFTA: every event draws its own samples, also when it
//! shares a `Model`, and every time gets fresh samples. The generator is
//! ftar's ([`sample`]), so sampled numbers match PFTA's in distribution, not
//! one by one; point values give the same numbers.

mod compute;
pub mod importance;
mod sample;
pub mod tsv;

pub use importance::Importance;

use std::collections::{HashMap, HashSet};

use crate::analysis::minimal_cut_sets;
use crate::error::{Error, Result};
use crate::model::Tree;
use crate::text::convert::{gate_trees, resolve};
use crate::text::{Ast, EventModel, GateType, Model, Param, Rate};
use compute::{Budget, Point, Term, Truncation};

/// Seed used when the file gives none (PFTA would then draw a random one;
/// ftar stays reproducible).
pub const DEFAULT_SEED: &str = "ftar";

/// Default limit on the combinations visited by the expansions.
pub const DEFAULT_BUDGET: u64 = 50_000_000;

/// Most `times × samples` a run may ask for.
pub const MAX_POINTS: usize = 10_000_000;

/// Settings that override the file's.
#[derive(Debug, Clone)]
pub struct QuantOptions {
    pub times: Option<Vec<f64>>,
    pub sample_size: Option<usize>,
    pub seed: Option<String>,
    pub computational_order: Option<usize>,
    pub computational_tolerance: Option<f64>,
    /// Compute the importance of each gate's events (three more expansions
    /// per event and point).
    pub importance: bool,
    pub budget: u64,
}

impl Default for QuantOptions {
    fn default() -> Self {
        Self {
            times: None,
            sample_size: None,
            seed: None,
            computational_order: None,
            computational_tolerance: None,
            importance: true,
            budget: DEFAULT_BUDGET,
        }
    }
}

/// Probability, intensity and rate at every point (time-major: point
/// `time_index * sample_size + sample`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Series {
    pub q: Vec<f64>,
    pub w: Vec<f64>,
    pub rate: Vec<f64>,
}

impl Series {
    fn from_qw(q: Vec<f64>, w: Vec<f64>) -> Self {
        let rate = q
            .iter()
            .zip(&w)
            .map(|(q, w)| compute::divide(*w, 1.0 - q))
            .collect();
        Series { q, w, rate }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModelRow {
    pub id: String,
    pub label: Option<String>,
    pub is_used: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EventResult {
    pub id: String,
    pub label: Option<String>,
    pub is_used: bool,
    pub series: Series,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CutSetResult {
    pub events: Vec<String>,
    pub series: Series,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GateResult {
    pub id: String,
    pub label: Option<String>,
    pub is_top_gate: bool,
    pub is_paged: bool,
    pub gate_type: GateType,
    pub inputs: Vec<String>,
    pub series: Series,
    pub cut_sets: Vec<CutSetResult>,
    /// The importance of each event in the gate's cut sets (empty unless
    /// [`QuantOptions::importance`]).
    pub importances: Vec<Importance>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct QuantReport {
    pub times: Vec<f64>,
    pub time_unit: Option<String>,
    pub sample_size: usize,
    pub seed: String,
    pub models: Vec<ModelRow>,
    pub events: Vec<EventResult>,
    pub gates: Vec<GateResult>,
}

impl QuantReport {
    /// Mean over the samples, for each time.
    pub fn expected(&self, values: &[f64]) -> Vec<f64> {
        let s = self.sample_size.max(1);
        values
            .chunks(s)
            .map(|c| c.iter().sum::<f64>() / c.len() as f64)
            .collect()
    }

    /// The `p`-quantile (0..=1) over the samples, for each time, ignoring `nan`.
    pub fn quantile(&self, values: &[f64], p: f64) -> Vec<f64> {
        values
            .chunks(self.sample_size.max(1))
            .map(|c| {
                let mut v: Vec<f64> = c.iter().copied().filter(|x| !x.is_nan()).collect();
                v.sort_by(f64::total_cmp);
                let pos = (p.clamp(0.0, 1.0) * (v.len().saturating_sub(1)) as f64).round();
                v.get(pos as usize).copied().unwrap_or(f64::NAN)
            })
            .collect()
    }
}

/// Settings after overrides.
struct Setup {
    times: Vec<f64>,
    samples: usize,
    seed: String,
    trunc: Truncation,
}

/// At most [`MAX_POINTS`] times × samples.
fn check_points(times: usize, samples: usize) -> Result<()> {
    if times.saturating_mul(samples) > MAX_POINTS {
        return Err(Error::Unsupported(format!(
            "times × samples is more than {MAX_POINTS}"
        )));
    }
    Ok(())
}

fn setup(ast: &Ast, o: &QuantOptions) -> Result<Setup> {
    let p = &ast.props;
    let times = o
        .times
        .clone()
        .or_else(|| p.times.clone())
        .unwrap_or_else(|| vec![1.0]);
    if times.is_empty() || times.iter().any(|t| *t < 0.0) {
        return Err(Error::Parse("times must be given and not negative".into()));
    }
    let samples = o.sample_size.or(p.sample_size).unwrap_or(1).max(1);
    check_points(times.len(), samples)?;
    let tolerance = o
        .computational_tolerance
        .or(p.computational_tolerance)
        .unwrap_or(0.0);
    if !(0.0..1.0).contains(&tolerance) {
        return Err(Error::Parse(
            "computational_tolerance must be within 0..1".into(),
        ));
    }
    Ok(Setup {
        times,
        samples,
        seed: o
            .seed
            .clone()
            .or_else(|| p.seed.clone())
            .unwrap_or_else(|| DEFAULT_SEED.into()),
        trunc: Truncation {
            order: o.computational_order.or(p.computational_order),
            tolerance,
        },
    })
}

// ------------------------------------------------------------- events

fn rate_samples(r: &Rate, n: usize, rng: &mut sample::Rng, id: &str) -> Result<Vec<f64>> {
    let (p, key, invert) = match r {
        Rate::Rate(p) => (p, "rate", false),
        Rate::MeanTime(p) => (p, "mean time", true),
    };
    let xs = draw(p, n, rng, id, key, false)?;
    Ok(if invert {
        xs.into_iter()
            .map(|m| if m == 0.0 { f64::INFINITY } else { 1.0 / m })
            .collect()
    } else {
        xs
    })
}

/// `n` samples of `p`; a probability must stay within 0..1 and anything else
/// non-negative (`nan` passes, as in PFTA).
fn draw(
    p: &Param,
    n: usize,
    rng: &mut sample::Rng,
    id: &str,
    key: &str,
    probability: bool,
) -> Result<Vec<f64>> {
    let xs: Vec<f64> = (0..n).map(|_| rng.sample(p)).collect();
    let bad = xs.iter().find(|&&x| {
        if probability {
            !(x.is_nan() || (0.0..=1.0).contains(&x))
        } else {
            x < 0.0
        }
    });
    if let Some(x) = bad {
        return Err(Error::Parse(format!(
            "event `{id}`: sampled {key} {x} is out of range"
        )));
    }
    Ok(xs)
}

/// `q(t)` for constant failure and repair rates, with PFTA's answers for
/// zero, infinite and `nan` rates and times.
fn constant_rate_q(t: f64, l: f64, m: f64) -> f64 {
    if l == 0.0 {
        let undefined = (m == 0.0 || m.is_nan()) && (t.is_infinite() || t.is_nan());
        return if undefined { f64::NAN } else { 0.0 };
    }
    if l.is_infinite() {
        let undefined = m.is_infinite() || m.is_nan() || t == 0.0 || t.is_nan();
        return if undefined { f64::NAN } else { 1.0 };
    }
    // A `nan` repair rate gives `nan` through the formula; a `nan` failure
    // rate must not reach the infinite-repair answer below.
    if l.is_nan() {
        return f64::NAN;
    }
    if m.is_infinite() {
        return 0.0;
    }
    l / (l + m) * -(-(l + m) * t).exp_m1()
}

/// `ω(t) = λ (1 - q(t))`, with PFTA's answers at the edges.
fn constant_rate_w(t: f64, l: f64, m: f64) -> f64 {
    if l == 0.0 {
        return 0.0;
    }
    if l.is_infinite() {
        let undefined = m.is_infinite() || m.is_nan() || t == 0.0 || t.is_nan();
        return if undefined { f64::NAN } else { m };
    }
    // `nan` rates give `nan` through `l` or the formula.
    if m.is_infinite() {
        return l;
    }
    l * (1.0 - constant_rate_q(t, l, m))
}

/// `q` and `ω` of one event at every point.
fn event_series(
    model: Option<&Model>,
    s: &Setup,
    rng: &mut sample::Rng,
    id: &str,
) -> Result<(Vec<f64>, Vec<f64>)> {
    let n = s.times.len() * s.samples;
    let time = |i: usize| s.times.get(i / s.samples).copied().unwrap_or(f64::NAN);
    Ok(match model {
        None => (vec![f64::NAN; n], vec![f64::NAN; n]),
        Some(Model::True) => (vec![1.0; n], vec![0.0; n]),
        Some(Model::False) => (vec![0.0; n], vec![0.0; n]),
        Some(Model::Fixed {
            probability,
            intensity,
        }) => (
            draw(probability, n, rng, id, "probability", true)?,
            draw(intensity, n, rng, id, "intensity", false)?,
        ),
        Some(Model::ConstantRate { failure, repair }) => {
            let l = rate_samples(failure, n, rng, id)?;
            let m = rate_samples(repair, n, rng, id)?;
            let lm = || l.iter().zip(&m).enumerate();
            let q = lm()
                .map(|(i, (l, m))| constant_rate_q(time(i), *l, *m))
                .collect();
            let w = lm()
                .map(|(i, (l, m))| constant_rate_w(time(i), *l, *m))
                .collect();
            (q, w)
        }
    })
}

// ------------------------------------------------------------- gates

/// The minimal cut sets of a gate's tree, as event indices, with `True`
/// events removed and sets with a `False` event dropped.
fn gate_terms(
    root: crate::model::Node,
    index: &HashMap<&str, u32>,
    constant: &HashMap<u32, bool>,
) -> Result<Vec<Term>> {
    let tree = Tree {
        db_name: None,
        note: String::new(),
        root,
    };
    let mut out = Vec::new();
    'sets: for cs in minimal_cut_sets(&tree)? {
        let mut t = Term::new();
        for id in cs.iter() {
            let e = *index
                .get(id)
                .ok_or_else(|| Error::MissingEvent(id.to_string()))?;
            match constant.get(&e) {
                Some(true) => {}
                Some(false) => continue 'sets,
                None => t.push(e),
            }
        }
        t.sort_unstable();
        t.dedup();
        out.push(t);
    }
    Ok(compute::minimise(out))
}

fn gate_series(
    terms: &[Term],
    events: &[EventResult],
    n: usize,
    trunc: Truncation,
    budget: &mut Budget,
) -> Result<(Series, Vec<CutSetResult>)> {
    let mut q = Vec::with_capacity(n);
    let mut w = Vec::with_capacity(n);
    let mut cuts: Vec<(Vec<f64>, Vec<f64>)> = vec![(Vec::new(), Vec::new()); terms.len()];
    for i in 0..n {
        let (eq, ew) = importance::point_values(events, i);
        let p = Point { q: &eq, w: &ew };
        q.push(compute::probability(terms, &p, trunc, budget)?);
        w.push(compute::intensity(terms, &p, trunc, budget)?);
        for (t, c) in terms.iter().zip(cuts.iter_mut()) {
            c.0.push(p.term_q(t));
            c.1.push(p.term_w(t));
        }
    }
    let names = |t: &Term| -> Vec<String> {
        t.iter()
            .filter_map(|&e| events.get(e as usize).map(|x| x.id.clone()))
            .collect()
    };
    let cut_sets = terms
        .iter()
        .zip(cuts)
        .map(|(t, (cq, cw))| CutSetResult {
            events: names(t),
            series: Series::from_qw(cq, cw),
        })
        .collect();
    Ok((Series::from_qw(q, w), cut_sets))
}

/// Quantify every event and gate of a file.
pub fn analyse(ast: &Ast, opts: &QuantOptions) -> Result<QuantReport> {
    let s = setup(ast, opts)?;
    let trees = gate_trees(ast)?;
    let n = s.times.len() * s.samples;
    let mut rng = sample::Rng::from_seed(&s.seed);
    let inputs: HashSet<&str> = ast
        .gates
        .iter()
        .flat_map(|g| g.inputs.iter().map(String::as_str))
        .collect();
    let mut events = Vec::with_capacity(ast.events.len());
    let mut constant = HashMap::new();
    for (i, e) in ast.events.iter().enumerate() {
        let model = e.model.as_ref().and_then(|m| resolve(m, &ast.models));
        match model {
            Some(Model::True) => constant.insert(i as u32, true),
            Some(Model::False) => constant.insert(i as u32, false),
            _ => None,
        };
        let (q, w) = event_series(model, &s, &mut rng, &e.id)?;
        events.push(EventResult {
            id: e.id.clone(),
            label: e.label.clone(),
            is_used: inputs.contains(e.id.as_str()),
            series: Series::from_qw(q, w),
        });
    }
    let index: HashMap<&str, u32> = ast
        .events
        .iter()
        .enumerate()
        .map(|(i, e)| (e.id.as_str(), i as u32))
        .collect();
    let mut budget = Budget(opts.budget);
    let mut gates = Vec::with_capacity(ast.gates.len());
    for (g, root) in ast.gates.iter().zip(trees) {
        let terms = gate_terms(root, &index, &constant)?;
        check_models(&terms, ast)?;
        let (series, cut_sets) = gate_series(&terms, &events, n, s.trunc, &mut budget)?;
        let importances = if opts.importance {
            importance::importances(&terms, &events, &series.q, s.trunc, &mut budget)?
        } else {
            Vec::new()
        };
        gates.push(GateResult {
            id: g.id.clone(),
            label: g.label.clone(),
            is_top_gate: !inputs.contains(g.id.as_str()),
            is_paged: g.is_paged,
            gate_type: g.gate_type,
            inputs: g.inputs.clone(),
            series,
            cut_sets,
            importances,
        });
    }
    let used: HashSet<&str> = ast
        .events
        .iter()
        .filter_map(|e| match &e.model {
            Some(EventModel::Shared(m)) => Some(m.as_str()),
            _ => None,
        })
        .collect();
    Ok(QuantReport {
        times: s.times,
        time_unit: ast.props.time_unit.clone(),
        sample_size: s.samples,
        seed: s.seed,
        models: ast
            .models
            .iter()
            .map(|m| ModelRow {
                id: m.id.clone(),
                label: m.label.clone(),
                is_used: used.contains(m.id.as_str()),
            })
            .collect(),
        events,
        gates,
    })
}

/// Events in a gate's cut sets need a model. (A transfer-in never gets
/// here: the cut-set engine refuses one that is not inlined.)
fn check_models(terms: &[Term], ast: &Ast) -> Result<()> {
    for e in terms.iter().flatten() {
        if let Some(ev) = ast.events.get(*e as usize) {
            if ev.model.is_none() {
                return Err(Error::Parse(format!(
                    "event `{}` has no probability or model",
                    ev.id
                )));
            }
        }
    }
    Ok(())
}

/// Quantify a document: its tree with transfer-ins inlined, its database
/// and extras (models, times, ...).
pub fn analyse_document(
    doc: &crate::project::Document,
    opts: &QuantOptions,
) -> Result<QuantReport> {
    let p = doc.analysis_project()?;
    let ast = crate::text::convert::from_document(&p.tree, &p.db, &doc.extras)?;
    analyse(&ast, opts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constant_rate_edges_follow_pfta() {
        let nan = f64::NAN;
        let inf = f64::INFINITY;
        assert_eq!(constant_rate_q(5.0, 0.0, 0.0), 0.0);
        assert!(constant_rate_q(inf, 0.0, 0.0).is_nan());
        assert_eq!(constant_rate_q(5.0, 0.0, inf), 0.0);
        assert_eq!(constant_rate_q(5.0, inf, 1.0), 1.0);
        assert!(constant_rate_q(0.0, inf, 1.0).is_nan());
        assert_eq!(constant_rate_q(5.0, 1.0, inf), 0.0);
        assert!(constant_rate_q(5.0, nan, 1.0).is_nan());
        let q = constant_rate_q(2.0, 0.5, 0.0);
        assert!((q - (1.0 - (-1.0f64).exp())).abs() < 1e-15);
        assert_eq!(constant_rate_w(5.0, 0.0, nan), 0.0);
        assert_eq!(constant_rate_w(5.0, inf, 3.0), 3.0);
        assert_eq!(constant_rate_w(5.0, 2.0, inf), 2.0);
        let w = constant_rate_w(2.0, 0.5, 0.0);
        assert!((w - 0.5 * (-1.0f64).exp()).abs() < 1e-15);
        // Repairable: steady state λ/(λ+μ).
        assert!((constant_rate_q(1e9, 1.0, 3.0) - 0.25).abs() < 1e-15);
    }

    /// PFTA's table of edge cases, row by row (`constant_rate_model_*`).
    #[test]
    fn constant_rate_edge_table() {
        let nan = f64::NAN;
        let inf = f64::INFINITY;
        let same = |a: f64, b: f64| a == b || (a.is_nan() && b.is_nan());
        // (t, λ, μ, q, ω)
        let rows = [
            (inf, 0.0, 0.0, nan, 0.0),
            (nan, 0.0, 0.0, nan, 0.0),
            (5.0, 0.0, 0.0, 0.0, 0.0),
            (inf, 0.0, nan, nan, 0.0),
            (5.0, 0.0, nan, 0.0, 0.0),
            (inf, 0.0, 1.0, 0.0, 0.0),
            (nan, 0.0, inf, 0.0, 0.0),
            (5.0, inf, inf, nan, nan),
            (5.0, inf, nan, nan, nan),
            (0.0, inf, 1.0, nan, nan),
            (nan, inf, 1.0, nan, nan),
            (5.0, inf, 1.0, 1.0, 1.0),
            (5.0, nan, 1.0, nan, nan),
            (5.0, 1.0, nan, nan, nan),
            (5.0, 1.0, inf, 0.0, 1.0),
            (5.0, nan, inf, nan, nan),
            (5.0, nan, 0.0, nan, nan),
        ];
        for (t, l, m, q, w) in rows {
            assert!(same(constant_rate_q(t, l, m), q), "q({t}, {l}, {m})");
            assert!(same(constant_rate_w(t, l, m), w), "ω({t}, {l}, {m})");
        }
    }

    #[test]
    fn rate_divides_by_the_chance_of_working() {
        let s = Series::from_qw(vec![0.5, 0.0, 1.0], vec![1.0, 2.0, 0.0]);
        assert_eq!(s.rate[..2], [2.0, 2.0]);
        assert!(s.rate[2].is_nan(), "0/0");
    }

    #[test]
    fn the_size_limit_is_exact() {
        assert!(check_points(MAX_POINTS, 1).is_ok());
        assert!(check_points(1, MAX_POINTS).is_ok());
        assert!(check_points(MAX_POINTS, 2).is_err());
        assert!(check_points(1, MAX_POINTS + 1).is_err());
    }

    #[test]
    fn times_must_be_given_and_not_negative() {
        let ast = crate::text::parse_text_ast(
            b"Gate: T
",
        )
        .unwrap();
        for times in [vec![], vec![1.0, -2.0], vec![-1.0]] {
            let o = QuantOptions {
                times: Some(times.clone()),
                ..QuantOptions::default()
            };
            assert!(setup(&ast, &o).is_err(), "{times:?}");
        }
        let o = QuantOptions {
            times: Some(vec![0.0, f64::NAN]),
            ..QuantOptions::default()
        };
        assert!(setup(&ast, &o).is_ok(), "0 and nan are times");
    }
}
