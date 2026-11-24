//! OpenFTA-style text reports and the failure statistics of the Monte Carlo analysis.
//!
//! # Failure statistics
//!
//! In the original, `FailureStatistics.c` is not a reliability-statistics
//! (MTTF / failure-rate) module. It is the bookkeeping behind the Monte Carlo
//! report: every simulated system failure is a *failure vector* (the set of
//! primary events that had failed); the module counts how often each distinct
//! vector occurred, keeps them ranked by count, and can *compress* them to
//! minimal sets. [`FailureStatistics`] ports exactly that:
//!
//! * [`FailureStatistics::record`] = `record_failure`. A new mode is put at the
//!   bottom of the rank list with count 1; when a count grows above the count
//!   of the mode ranked directly above it, the two swap (strict `>`, so among
//!   equal counts the mode that got there first stays on top).
//! * `max_fail` = `initialise_failures(n)`. `0` means unlimited. As in the C
//!   code the test is `nfm <= max_fail`, so `max_fail + 1` modes are kept; further
//!   new modes only increment [`FailureStatistics::other`].
//! * [`FailureStatistics::compress`] = `compress_fail_data2`: modes are put in
//!   increasing order of size (stable; within a size by increasing count, the
//!   order of the rank list from its low end). Then, for each mode `p` in that
//!   order, every earlier surviving mode `q` that is a subset of `p` absorbs
//!   it (`q.count += p.count`, once per absorbing `q`, as in C) and `p` is
//!   dropped. Output order is the same increasing-size order.
//!
//! Derived numbers used in the Monte Carlo report (`bas_prob` is the
//! probability that at least one component fails):
//!
//! * mode probability  `bas_prob * n / trials` (± `bas_prob * sqrt(n) / trials`)
//! * mode importance   `100 * n / system_failures` percent
//! * event contribution = sum of the mode probabilities of the (compressed)
//!   modes containing the event; event importance = `100 * contribution / P(top)`.
//!
//! The Rust simulation ([`crate::montecarlo::simulate`]) uses the same
//! first-failure scheme as the C "fast method" by default, so `bas_prob` is the
//! probability of at least one component failure and the "only runs with at
//! least one component failure" note is printed. With
//! [`crate::montecarlo::Method::Direct`] `bas_prob = 1`.
//! [`failure_statistics_from_modes`] builds the statistics from the recorded
//! failure vectors; [`failure_statistics_from_hits`] is the older approximation
//! from per-cut-set hit counts.
//!
//! # Reports
//!
//! All generators return a `String` and take the `Time   :` text as an argument
//! (C `ctime` without the trailing newline, e.g. `"Mon Jan  1 12:00:00 2024"`).
//! Number formatting follows C: `%E` is [`fmt_e`], `%f` is `{:.6}`.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::analysis::{CutSet, ProbabilityOptions, ProbabilityReport};
use crate::model::{PrimaryEventDb, Tree};
use crate::montecarlo::{MonteCarloOptions, MonteCarloReport};

// ------------------------------------------------------------------ helpers

/// C `printf("%E")`: six decimals and an exponent with sign and at least two digits.
pub fn fmt_e(x: f64) -> String {
    if x.is_nan() {
        return "NAN".into();
    }
    if x.is_infinite() {
        return if x < 0.0 { "-INF".into() } else { "INF".into() };
    }
    let s = format!("{x:.6E}");
    let (m, e) = s.split_once('E').expect("exponent");
    let e: i32 = e.parse().expect("exponent value");
    format!("{m}E{}{:02}", if e < 0 { '-' } else { '+' }, e.abs())
}

/// Primary event ids in the order of OpenFTA's event list (alphabetical, see
/// [`Tree::primary_event_ids`]); reports list events in this order.
pub fn primary_event_order(tree: &Tree) -> Vec<String> {
    tree.primary_event_ids()
}

/// `s` padded with spaces to `width` *bytes*, as C's `%-Ns` does (Rust's
/// `{:<N}` counts characters, so ids with letters such as `ä` would line up
/// differently from OpenFTA's reports).
fn pad(s: &str, width: usize) -> String {
    let mut out = String::with_capacity(width.max(s.len()));
    out.push_str(s);
    out.extend(std::iter::repeat_n(' ', width.saturating_sub(s.len())));
    out
}

/// Port of `BitPara`/`ParaGrow`: event ids (in `events` order) as lines of at
/// most `width` characters, every id preceded by a space.
fn paragraph(set: &CutSet, events: &[String], width: usize) -> Vec<String> {
    let mut ids: Vec<&str> = events
        .iter()
        .filter(|e| set.contains(e))
        .map(String::as_str)
        .collect();
    // Events missing from the order list are appended alphabetically rather than lost.
    ids.extend(set.iter().filter(|e| !events.iter().any(|x| x == e)));
    let mut lines: Vec<String> = Vec::new();
    for id in ids {
        let s = format!(" {id}");
        match lines.last_mut() {
            Some(l) if l.len() + s.len() <= width => l.push_str(&s),
            _ => lines.push(s),
        }
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

fn header(out: &mut String, title: &str, tree_name: &str, time: &str) {
    let _ = write!(out, "{title}\n{}\n\n", "=".repeat(title.len()));
    let _ = writeln!(out, "Tree   : {tree_name}");
    // ctime() ends in a newline, so the original has a blank line after it.
    let _ = write!(out, "Time   : {time}\n\n");
}

// ------------------------------------------------------- failure statistics

/// A failure mode (set of failed primary events) with its number of occurrences.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FailureMode {
    pub events: CutSet,
    pub count: u64,
}

/// Ranked failure-mode counts (port of `FailureStatistics.c`).
#[derive(Debug, Clone, Default)]
pub struct FailureStatistics {
    max_fail: usize,
    nodes: Vec<FailureMode>,
    /// Node ids from the low end (newest / smallest counts) to the high end of the rank list.
    order: Vec<usize>,
    /// Position of every node in `order`.
    pos: Vec<usize>,
    index: BTreeMap<CutSet, usize>,
    other: u64,
}

impl FailureStatistics {
    /// `initialise_failures(max_fail)`; `0` = unlimited.
    pub fn new(max_fail: usize) -> Self {
        Self {
            max_fail,
            ..Self::default()
        }
    }

    /// `record_failure`: count one more occurrence of `failure`.
    pub fn record(&mut self, failure: &CutSet) {
        self.record_n(failure, 1);
    }

    /// Count `n` occurrences at once (same final counts as `n` calls of [`record`](Self::record);
    /// tie order may differ).
    pub fn record_n(&mut self, failure: &CutSet, n: u64) {
        if n == 0 {
            return;
        }
        let id = match self.index.get(failure) {
            Some(&id) => {
                self.nodes[id].count += n;
                id
            }
            None => {
                // C quirk: `nfm <= max_fail` keeps max_fail + 1 modes.
                if self.max_fail != 0 && self.nodes.len() > self.max_fail {
                    self.other += n;
                    return;
                }
                let id = self.nodes.len();
                self.nodes.push(FailureMode {
                    events: failure.clone(),
                    count: n,
                });
                self.index.insert(failure.clone(), id);
                self.order.insert(0, id);
                self.pos.push(0);
                self.reindex();
                id
            }
        };
        // Bubble towards the high end while strictly larger than the neighbour above.
        let mut p = self.pos[id];
        while p + 1 < self.order.len() && self.nodes[id].count > self.nodes[self.order[p + 1]].count
        {
            self.order.swap(p, p + 1);
            self.pos[self.order[p]] = p;
            p += 1;
        }
        self.pos[id] = p;
    }

    fn reindex(&mut self) {
        for (i, &nid) in self.order.iter().enumerate() {
            self.pos[nid] = i;
        }
    }

    /// Number of distinct modes recorded (`nfm`).
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Failures of modes that could not be recorded because of `max_fail` (`nother`).
    pub fn other(&self) -> u64 {
        self.other
    }

    /// Modes ranked from most to least frequent (the order of `list_print`).
    pub fn ranked(&self) -> Vec<FailureMode> {
        self.order
            .iter()
            .rev()
            .map(|&i| self.nodes[i].clone())
            .collect()
    }

    /// `compress_fail_data2`: absorb modes that contain another mode (see module docs).
    pub fn compress(&self) -> Vec<FailureMode> {
        let mut modes: Vec<FailureMode> =
            self.order.iter().map(|&i| self.nodes[i].clone()).collect();
        modes.sort_by_key(|m| m.events.len()); // stable: keeps low-to-high rank order inside a size
        let mut alive = vec![true; modes.len()];
        for p in 0..modes.len() {
            let mut absorbed = false;
            for q in 0..p {
                if alive[q] && modes[q].count != 0 && modes[q].events.is_subset(&modes[p].events) {
                    absorbed = true;
                    modes[q].count += modes[p].count;
                }
            }
            if absorbed {
                alive[p] = false;
            }
        }
        modes
            .into_iter()
            .zip(alive)
            .filter(|(_, a)| *a)
            .map(|(m, _)| m)
            .collect()
    }
}

/// Failure statistics from the per-cut-set hit counts of a Monte Carlo run
/// (cut sets that were never fully failed are skipped).
pub fn failure_statistics_from_hits(
    report: &MonteCarloReport,
    max_fail: usize,
) -> FailureStatistics {
    let mut fs = FailureStatistics::new(max_fail);
    for (cs, n) in &report.cut_set_hits {
        fs.record_n(cs, *n);
    }
    fs
}

/// Failure statistics from the recorded failure vectors of a Monte Carlo run
/// (OpenFTA's `record_failure`, unlimited like `MONTE_MAX_F = 0`).
pub fn failure_statistics_from_modes(report: &MonteCarloReport) -> FailureStatistics {
    let mut fs = FailureStatistics::new(1 << 30);
    for (cs, n) in &report.modes {
        fs.record_n(cs, *n);
    }
    fs
}

/// Contribution and importance of each event, computed from failure modes
/// (`MonteFileWriteImportance`). `scale` multiplies `count / trials`
/// (the C `bas_prob`). Returns `(event, contribution, importance %)` in `events` order.
pub fn event_contributions(
    events: &[String],
    modes: &[FailureMode],
    trials: u64,
    scale: f64,
    top_probability: f64,
) -> Vec<(String, f64, f64)> {
    events
        .iter()
        .map(|e| {
            let c: f64 = modes
                .iter()
                .filter(|m| m.events.contains(e))
                .map(|m| scale * m.count as f64 / trials as f64)
                .sum::<f64>()
                // An empty float sum is -0.0, which would print as "-0.000000E+00".
                + 0.0;
            // No simulated failures gives 0/0; show 0 % rather than "NaN%".
            let imp = if top_probability > 0.0 {
                100.0 * c / top_probability
            } else {
                0.0
            };
            (e.clone(), c, imp)
        })
        .collect()
}

// --------------------------------------------------------------- cut sets

/// `Minimal Cut Sets` report (`mcs_print` + header of the algebraic method).
///
/// `cut_sets` are grouped by order; `events` gives the primary event order
/// (see [`primary_event_order`]); `order_limit` is the order the cut sets were
/// limited to (`usize::MAX` or the number of events for all of them).
#[allow(clippy::needless_range_loop)]
pub fn cut_sets_report(
    tree_name: &str,
    time: &str,
    events: &[String],
    order_limit: usize,
    cut_sets: &[CutSet],
) -> String {
    let mut out = String::new();
    header(&mut out, "Minimal Cut Sets", tree_name, time);
    // OpenFTA prints the order limit the analysis ran with (`tree->max_order`)
    // and lists every order up to it, empty ones included (`mcs_print`). A
    // limit is never more than the number of events, nor less than what was found.
    let found = cut_sets.iter().map(|c| c.len()).max().unwrap_or(0);
    let max_order = order_limit.min(events.len()).max(found);
    let _ = writeln!(out, "Method : Algebraic\n");
    let _ = writeln!(out, "No. of primary events = {}", events.len());
    let _ = writeln!(out, "Minimal cut set order = 1 to {max_order}");

    let mut counts = vec![0usize; max_order + 1];
    for order in 1..=max_order {
        let _ = write!(out, "\nOrder {order}:\n");
        for cs in cut_sets.iter().filter(|c| c.len() == order) {
            counts[order] += 1;
            let lines = paragraph(cs, events, 50);
            let _ = writeln!(out, "  {:>3}) {}", counts[order], pad(&lines[0], 50));
            for l in &lines[1..] {
                let _ = writeln!(out, "       {}", pad(l, 50));
            }
        }
    }
    out.push_str("\n\nQualitative Importance Analysis:\n\n");
    out.push_str("Order        Number\n");
    out.push_str("-----        ------\n");
    for order in 1..=max_order {
        let _ = writeln!(out, "{order:>4}           {}", counts[order]);
    }
    let _ = write!(
        out,
        "  ALL          {}\n\n",
        counts[1..].iter().sum::<usize>()
    );
    out
}

// ---------------------------------------------------------- probabilities

/// `Probabilities Analysis` report (`calculate_probs`).
///
/// `tree_max_order` is the order limit the cut sets were generated with (the C
/// `tree->max_order`: the number of primary events when all cut sets are
/// generated); `report` must come from [`crate::analysis::probability_report`]
/// with `opts`.
pub fn probability_report_text(
    tree_name: &str,
    time: &str,
    events: &[String],
    tree_max_order: usize,
    opts: &ProbabilityOptions,
    report: &ProbabilityReport,
) -> String {
    let mut out = String::new();
    header(&mut out, "Probabilities Analysis", tree_name, time);
    let max_order = opts.max_order.min(tree_max_order);
    let _ = writeln!(out, "Number of primary events   = {}", report.events.len());
    let _ = writeln!(
        out,
        "Number of minimal cut sets = {}",
        report.cut_sets.len()
    );
    let _ = writeln!(out, "Order of minimal cut sets  = {tree_max_order}");
    if max_order < tree_max_order {
        let _ = write!(out, "               (order <= {max_order} used)\n\n");
    } else {
        out.push('\n');
    }
    let _ = write!(out, "Unit time span         = {:.6}\n\n", opts.unit_time);

    out.push_str("Minimal cut set probabilities :\n\n");
    for (i, (cs, p)) in report.cut_sets.iter().enumerate() {
        let lines = paragraph(cs, events, 30);
        let _ = writeln!(out, "{:>3}   {}   {}", i + 1, pad(&lines[0], 30), fmt_e(*p));
        for l in &lines[1..] {
            let _ = writeln!(out, "      {}", pad(l, 20));
        }
    }

    let _ = write!(
        out,
        "\n\nProbability of top level event (minimal cut sets up to order {max_order} used):\n\n"
    );
    for (k, (t, s)) in report.terms.iter().zip(&report.partial_sums).enumerate() {
        let i = k + 1;
        let _ = writeln!(
            out,
            "{:>2} term{}   {}{}   = {} ({} bound)",
            i,
            if i > 1 { "s" } else { " " },
            if i % 2 == 1 { "+" } else { "-" },
            fmt_e(t.abs()),
            fmt_e(*s),
            if i % 2 == 1 { "upper" } else { "lower" }
        );
    }
    if report.exact {
        let _ = write!(out, "\nExact value : {}\n", fmt_e(report.top));
    }

    out.push_str("\n\nPrimary Event Analysis:\n\n");
    out.push_str(" Event          Failure contrib.    Importance\n\n");
    for e in &report.events {
        let _ = writeln!(
            out,
            "{} {}            {:>5.2}%",
            pad(&format!(" {}", e.id), 15),
            fmt_e(e.contribution),
            e.importance_pct
        );
    }
    out
}

// ------------------------------------------------------------ Monte Carlo

fn mode_rows(
    out: &mut String,
    modes: &[FailureMode],
    events: &[String],
    trials: u64,
    failures: u64,
    scale: f64,
) {
    for (i, m) in modes.iter().enumerate() {
        let lines = paragraph(&m.events, events, 20);
        let n = m.count as f64;
        let _ = writeln!(
            out,
            "{:>3}   {}  {:<8}  {} ( +/- {} )  {:>5.2}%",
            i + 1,
            pad(&lines[0], 20),
            m.count,
            fmt_e(scale * n / trials as f64),
            fmt_e(scale * n.sqrt() / trials as f64),
            100.0 * n / failures as f64
        );
        for l in &lines[1..] {
            let _ = writeln!(out, "      {}", pad(l, 20));
        }
    }
}

/// `Monte Carlo Simulation` report (`MonteFileWrite`).
///
/// `stats` holds the recorded failure modes (e.g. from
/// [`failure_statistics_from_hits`]); event order is taken from
/// `report.event_failures`.
pub fn monte_carlo_report_text(
    tree_name: &str,
    time: &str,
    unit_time: f64,
    report: &MonteCarloReport,
    stats: &FailureStatistics,
) -> String {
    let mut out = String::new();
    header(&mut out, "Monte Carlo Simulation", tree_name, time);
    let events: Vec<String> = report
        .event_failures
        .iter()
        .map(|(e, _)| e.clone())
        .collect();
    if report.any_component_failure == 0.0 {
        out.push_str("All primary event probabilties are 0.0 - analysis abandoned\n\n");
        return out;
    }
    if report.method == crate::montecarlo::Method::FirstFailure {
        out.push_str("Note: Only runs with at least one component failure are simulated\n\n");
    }
    let _ = writeln!(out, "Number of primary events  = {}", events.len());
    let _ = writeln!(out, "Number of tests           = {}", report.trials);
    let _ = write!(out, "Unit Time span used       = {unit_time:.6}\n\n");
    let _ = write!(
        out,
        "Number of system failures = {}\n\n",
        report.system_failures
    );
    let _ = writeln!(
        out,
        "Probability of at least   = {}  ( exact )",
        fmt_e(report.any_component_failure)
    );
    out.push_str("one component failure\n\n");
    let p = report.probability;
    let err = report.std_error;
    let _ = match report.upper_bound_95() {
        Some(b) => write!(
            out,
            "Probability of top event  = {}  ( no failures simulated; below {} at 95% )\n\n",
            fmt_e(p),
            fmt_e(b)
        ),
        None => write!(
            out,
            "Probability of top event  = {}  ( +/- {} )\n\n",
            fmt_e(p),
            fmt_e(err)
        ),
    };

    if report.system_failures == 0 {
        out.push_str(
            "No system failure occurred in the simulation, so no failure modes are listed.\n\
             The top event is rare: increase the number of tests, or use the\n\
             cut set or probability analysis for an exact result.\n\n",
        );
    }
    out.push_str(
        "Rank   Failure mode         Failures  Estimated Probability         Importance\n\n",
    );
    mode_rows(
        &mut out,
        &stats.ranked(),
        &events,
        report.trials,
        report.system_failures,
        report.bas_prob,
    );
    if stats.other() > 0 {
        let _ = writeln!(
            out,
            "other failures ( mode not recorded ) {:<8}  {}",
            stats.other(),
            fmt_e(report.bas_prob * stats.other() as f64 / report.trials as f64)
        );
    }

    let compressed = stats.compress();
    out.push_str("\n\nCompressed:\n\n");
    out.push_str("Rank   Failure mode         Failures  Estimated Probability    Importance\n\n");
    mode_rows(
        &mut out,
        &compressed,
        &events,
        report.trials,
        report.system_failures,
        report.bas_prob,
    );

    out.push_str("\n\nPrimary Event Analysis:\n\n");
    out.push_str(" Event          Failure contrib.    Importance\n\n");
    for (e, c, imp) in event_contributions(&events, &compressed, report.trials, report.bas_prob, p)
    {
        let _ = writeln!(
            out,
            "{} {}            {:>5.2}%",
            pad(&format!(" {e}"), 15),
            fmt_e(c),
            imp
        );
    }
    out
}

// -------------------------------------------------------------- statistics

/// Plain listing of the ranked failure modes (`list_print`) with a summary.
pub fn failure_statistics_report(tree_name: &str, time: &str, stats: &FailureStatistics) -> String {
    let mut out = String::new();
    header(&mut out, "Failure Statistics", tree_name, time);
    let _ = writeln!(out, "Number of failure modes   = {}", stats.len());
    let _ = write!(out, "Other failures (not kept) = {}\n\n", stats.other());
    out.push_str("Ranked failure modes:\n\n");
    for (i, m) in stats.ranked().iter().enumerate() {
        let ids = m.events.join(" ");
        let _ = writeln!(out, "({:<3}) {} - {}", i, ids, m.count);
    }
    out.push_str("\nCompressed (minimal) modes:\n\n");
    for (i, m) in stats.compress().iter().enumerate() {
        let ids = m.events.join(" ");
        let _ = writeln!(out, "({:<3}) {} - {}", i, ids, m.count);
    }
    out
}

// ------------------------------------------------------------------ generate

/// Which text report to produce.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportKind {
    CutSets,
    Probability,
    MonteCarlo,
    FailureStatistics,
}

impl ReportKind {
    pub const ALL: [ReportKind; 4] = [
        ReportKind::CutSets,
        ReportKind::Probability,
        ReportKind::MonteCarlo,
        ReportKind::FailureStatistics,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ReportKind::CutSets => "Cut sets",
            ReportKind::Probability => "Probability",
            ReportKind::MonteCarlo => "Monte Carlo",
            ReportKind::FailureStatistics => "Failure statistics",
        }
    }

    /// Short file-name friendly name.
    pub fn slug(self) -> &'static str {
        match self {
            ReportKind::CutSets => "cutsets",
            ReportKind::Probability => "probability",
            ReportKind::MonteCarlo => "montecarlo",
            ReportKind::FailureStatistics => "failure_statistics",
        }
    }

    pub fn from_slug(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.slug() == s)
    }
}

/// Parameters of the analyses behind the reports.
#[derive(Debug, Clone)]
pub struct ReportParams {
    pub max_order: usize,
    pub terms: usize,
    pub monte_carlo: MonteCarloOptions,
}

impl Default for ReportParams {
    fn default() -> Self {
        Self {
            max_order: usize::MAX,
            terms: ProbabilityOptions::default().terms,
            monte_carlo: MonteCarloOptions::default(),
        }
    }
}

/// Run the analysis behind `kind` and format its OpenFTA-style text report.
/// `time` is the text for the `Time   :` header line (pass `""` for reproducible output).
pub fn generate(
    kind: ReportKind,
    tree: &Tree,
    db: &PrimaryEventDb,
    tree_name: &str,
    time: &str,
    params: &ReportParams,
) -> crate::Result<String> {
    let events = primary_event_order(tree);
    Ok(match kind {
        ReportKind::CutSets => {
            let sets = crate::analysis::minimal_cut_sets_limited(tree, params.max_order)?;
            cut_sets_report(tree_name, time, &events, params.max_order, &sets)
        }
        ReportKind::Probability => {
            let opts = ProbabilityOptions {
                max_order: params.max_order,
                terms: params.terms,
                unit_time: params.monte_carlo.unit_time,
            };
            let r = crate::analysis::probability_report(tree, db, &opts)?;
            // OpenFTA prints the order limit the cut sets were generated with
            // (`tree->max_order`), not the largest order found: "generate all"
            // is the number of primary events, and a limit is at most that.
            let order = params.max_order.min(r.events.len());
            probability_report_text(tree_name, time, &events, order, &opts, &r)
        }
        ReportKind::MonteCarlo | ReportKind::FailureStatistics => {
            let r = crate::montecarlo::simulate(tree, db, &params.monte_carlo)?;
            let stats = failure_statistics_from_modes(&r);
            if kind == ReportKind::MonteCarlo {
                monte_carlo_report_text(tree_name, time, params.monte_carlo.unit_time, &r, &stats)
            } else {
                failure_statistics_report(tree_name, time, &stats)
            }
        }
    })
}
