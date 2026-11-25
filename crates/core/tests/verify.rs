//! Independent brute-force verification of the analysis engine.
//!
//! The oracle evaluates the fault-tree boolean function by truth table over the
//! primary events, mirroring `eval_tree` in OpenFTA's `treeUtil.c`:
//!
//! * intermediate events and transfer-out markers are transparent (the C
//!   "processing pointers" skip them),
//! * AND / INHIBIT / PRIORITY-AND = conjunction, OR / XOR = disjunction,
//! * an analysed condition as first child of an OR/XOR must hold (it is ANDed
//!   with the rest), a not-analysed condition as first child is ignored,
//! * a transfer-in evaluates the head of the transferred tree.
//!
//! Everything derived (minimal cut sets, exact probability, importance) is
//! recomputed from the truth table and compared with the library.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use ftar_core::analysis::{
    minimal_cut_sets, minimal_cut_sets_limited, probability_report, CutSet, ProbabilityOptions,
};
use ftar_core::model::{EventType, Node};
use ftar_core::montecarlo::{simulate, Method, MonteCarloOptions};
use ftar_core::parse::parse_fta;
use ftar_core::project::Project;

// ------------------------------------------------------------------ PRNG

struct Prng(u64);
impl Prng {
    fn next(&mut self) -> u64 {
        // splitmix64
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    fn chance(&mut self, pct: usize) -> bool {
        self.below(100) < pct
    }
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
}

// ---------------------------------------------------------------- oracle

type Resolver<'a> = &'a dyn Fn(&str) -> Node;

/// The children the C code sees: intermediates spliced in, transfer-outs dropped.
fn flatten(children: &[Node]) -> Vec<&Node> {
    let mut out = Vec::new();
    for c in children {
        match c.kind {
            EventType::Intermediate => out.extend(flatten(&c.children)),
            EventType::TransferOut => {}
            _ => out.push(c),
        }
    }
    out
}

/// The process tree: what `eval_tree` walks, with transfers resolved.
enum Proc {
    Leaf(String),
    True,
    Or {
        cond: Option<Box<Proc>>,
        kids: Vec<Proc>,
    },
    And(Vec<Proc>),
}

fn compile(n: &Node, res: Resolver) -> Proc {
    use EventType::*;
    match n.kind {
        Basic | Undeveloped | External | Dormant | ConditionAnalysed => Proc::Leaf(n.id.clone()),
        ConditionNotAnalysed => Proc::True,
        Or | Xor => {
            let ch = flatten(&n.children);
            let mut i = 0;
            let mut cond = None;
            if ch[0].kind == ConditionAnalysed {
                cond = Some(Box::new(compile(ch[0], res)));
                i = 1;
            }
            if ch.get(i).is_some_and(|c| c.kind == ConditionNotAnalysed) {
                i += 1;
            }
            Proc::Or {
                cond,
                kids: ch[i..].iter().map(|c| compile(c, res)).collect(),
            }
        }
        And | Inhibit | PriorityAnd => {
            let ch = flatten(&n.children);
            let i = usize::from(ch[0].kind == ConditionNotAnalysed);
            Proc::And(ch[i..].iter().map(|c| compile(c, res)).collect())
        }
        TransferIn => {
            let sub = res(&n.id);
            compile(flatten(std::slice::from_ref(&sub))[0], res)
        }
        Intermediate | TransferOut | Unknown => {
            panic!("unexpected node {:?} in process tree", n.kind)
        }
    }
}

fn eval(p: &Proc, val: &dyn Fn(&str) -> bool) -> bool {
    match p {
        Proc::Leaf(id) => val(id),
        Proc::True => true,
        Proc::Or { cond, kids } => {
            cond.as_ref().is_none_or(|c| eval(c, val)) && kids.iter().any(|k| eval(k, val))
        }
        Proc::And(kids) => kids.iter().all(|k| eval(k, val)),
    }
}

fn collect_ids(n: &Node, res: Resolver, out: &mut BTreeSet<String>) {
    use EventType::*;
    match n.kind {
        Basic | Undeveloped | External | Dormant | ConditionAnalysed => {
            out.insert(n.id.clone());
        }
        TransferIn => collect_ids(&res(&n.id), res, out),
        _ => {}
    }
    for c in &n.children {
        collect_ids(c, res, out);
    }
}

struct Oracle {
    ids: Vec<String>,
    truth: Vec<bool>,
}

impl Oracle {
    fn new(root: &Node, res: Resolver) -> Oracle {
        let mut set = BTreeSet::new();
        collect_ids(root, res, &mut set);
        let ids: Vec<String> = set.into_iter().collect();
        assert!(ids.len() <= 22, "oracle limited to 22 events");
        let index: HashMap<&str, usize> = ids
            .iter()
            .enumerate()
            .map(|(i, x)| (x.as_str(), i))
            .collect();
        let top = compile(flatten(std::slice::from_ref(root))[0], res);
        let truth = (0..1usize << ids.len())
            .map(|mask| {
                let val = |id: &str| mask >> index[id] & 1 == 1;
                eval(&top, &val)
            })
            .collect();
        Oracle { ids, truth }
    }

    /// Minimal true points of the function, as sets of event ids.
    fn min_cut_sets(&self) -> BTreeSet<CutSet> {
        let n = self.ids.len();
        // the function must be monotone for "minimal true point" to be meaningful
        for m in 0..self.truth.len() {
            if self.truth[m] {
                for b in 0..n {
                    assert!(self.truth[m | 1 << b], "function not monotone");
                }
            }
        }
        (1..self.truth.len())
            .filter(|&m| {
                self.truth[m] && (0..n).all(|b| m >> b & 1 == 0 || !self.truth[m ^ 1 << b])
            })
            .map(|m| self.mask_set(m))
            .collect()
    }

    fn mask_set(&self, m: usize) -> CutSet {
        (0..self.ids.len())
            .filter(|i| m >> i & 1 == 1)
            .map(|i| self.ids[i].clone())
            .collect()
    }

    fn mask_of(&self, cs: &CutSet) -> usize {
        cs.iter()
            .map(|id| 1usize << self.ids.iter().position(|x| x == id).unwrap())
            .sum()
    }

    /// Sum over the truth table of the probability of every state with `pred`.
    fn prob(&self, probs: &[f64], pred: impl Fn(usize) -> bool) -> f64 {
        (0..self.truth.len())
            .filter(|&m| pred(m))
            .map(|m| {
                (0..self.ids.len())
                    .map(|i| {
                        if m >> i & 1 == 1 {
                            probs[i]
                        } else {
                            1.0 - probs[i]
                        }
                    })
                    .product::<f64>()
            })
            .sum()
    }
}

type ProbMap = HashMap<String, (f64, bool)>;

/// P(failure within `t`): the value itself, or 1 - e^(-rate t) for `L` events.
fn eff(p: &ProbMap, id: &str, t: f64) -> f64 {
    let (v, l) = p[id];
    if l {
        1.0 - (-v * t).exp()
    } else {
        v
    }
}

fn close(a: f64, b: f64, what: &str, label: &str) {
    assert!(
        (a - b).abs() <= 1e-12 + 1e-9 * a.abs().max(b.abs()),
        "{label}: {what}: {a} vs {b}"
    );
}

/// Minimal cut sets, of every order and limited to orders 1 to 4, against the oracle's.
fn check_cut_sets(label: &str, proj: &Project, want: &BTreeSet<CutSet>) {
    let got: BTreeSet<CutSet> = minimal_cut_sets(&proj.tree).unwrap().into_iter().collect();
    assert_eq!(&got, want, "{label}: minimal cut sets");
    for lim in 1..=4 {
        let w: BTreeSet<CutSet> = want.iter().filter(|c| c.len() <= lim).cloned().collect();
        let g: BTreeSet<CutSet> = minimal_cut_sets_limited(&proj.tree, lim)
            .unwrap()
            .into_iter()
            .collect();
        assert_eq!(g, w, "{label}: cut sets limited to order {lim}");
    }
}

/// Compare every analysis result of `proj` against the oracle.
/// Returns the number of minimal cut sets.
fn check(
    label: &str,
    proj: &Project,
    root: &Node,
    res: Resolver,
    probs: &ProbMap,
    t: f64,
) -> usize {
    let o = Oracle::new(root, res);
    assert!(!o.truth[0], "{label}: top event true with no failures");
    let want = o.min_cut_sets();

    // (a) minimal cut sets == minimal true points
    check_cut_sets(label, proj, &want);

    // (b) exact probability, with and without an order limit
    let p: Vec<f64> = o.ids.iter().map(|id| eff(probs, id, t)).collect();
    let mut checked = 0;
    for lim in [usize::MAX, 1, 2, 3] {
        let sets: Vec<CutSet> = want.iter().filter(|c| c.len() <= lim).cloned().collect();
        if sets.len() > 16 {
            continue;
        }
        checked += 1;
        let masks: Vec<usize> = sets.iter().map(|c| o.mask_of(c)).collect();
        let exact = o.prob(&p, |m| masks.iter().copied().any(|c| m & c == c));
        let opts = ProbabilityOptions {
            max_order: lim,
            terms: usize::MAX,
            unit_time: t,
        };
        let r = probability_report(&proj.tree, &proj.db, &opts).unwrap();
        assert!(r.exact, "{label}: report not exact");
        close(
            r.top,
            exact,
            &format!("top probability (order {lim})"),
            label,
        );
        if lim == usize::MAX {
            // the function itself, not only its cut-set expansion
            close(
                r.top,
                o.prob(&p, |m| o.truth[m]),
                "top vs truth table",
                label,
            );
        }

        // inclusion-exclusion terms alternate between upper and lower bounds
        for (k, ps) in r.partial_sums.iter().enumerate() {
            if k % 2 == 0 {
                assert!(
                    *ps >= exact - 1e-12,
                    "{label}: partial {k} not an upper bound"
                );
            } else {
                assert!(
                    *ps <= exact + 1e-12,
                    "{label}: partial {k} not a lower bound"
                );
            }
        }
        assert_eq!(r.cut_sets.len(), sets.len(), "{label}: cut set count");

        // (c) importance: contribution = sum of probabilities of cut sets with the event
        let mut seen = BTreeSet::new();
        for e in &r.events {
            assert!(
                seen.insert(e.id.clone()),
                "{label}: event {} listed twice",
                e.id
            );
            let i = o
                .ids
                .iter()
                .position(|x| *x == e.id)
                .expect("event not in oracle");
            let contrib: f64 = masks
                .iter()
                .filter(|&&c| c >> i & 1 == 1)
                .map(|&c| cs_prob(&o, &p, c))
                .sum();
            close(
                e.contribution,
                contrib,
                &format!("contribution of {}", e.id),
                label,
            );
            let pct = if exact > 0.0 {
                100.0 * contrib / exact
            } else {
                0.0
            };
            close(
                e.importance_pct,
                pct,
                &format!("importance of {}", e.id),
                label,
            );
        }
        assert_eq!(
            seen.len(),
            o.ids.len(),
            "{label}: events in report vs oracle"
        );
    }
    assert!(checked > 0, "{label}: nothing checked");
    want.len()
}

/// Probability that all events of cut set `c` fail.
fn cs_prob(o: &Oracle, p: &[f64], c: usize) -> f64 {
    (0..o.ids.len())
        .filter(|i| c >> i & 1 == 1)
        .map(|i| p[i])
        .product()
}

// ------------------------------------------------------- random generation

fn node(kind: EventType, id: &str, children: Vec<Node>) -> Node {
    Node {
        kind,
        modifier: None,
        id: id.into(),
        text: String::new(),
        children,
    }
}

struct Gen<'a> {
    rng: &'a mut Prng,
    events: Vec<(String, EventType)>,
    conds: Vec<String>,
}

impl Gen<'_> {
    fn event(&mut self) -> Node {
        let (id, k) = self.events[self.rng.below(self.events.len())].clone();
        node(k, &id, vec![])
    }

    fn child(&mut self, depth: u32, targets: &[usize]) -> Node {
        let r = self.rng.below(100);
        if r < 50 || depth >= 3 {
            self.event()
        } else if r < 85 || targets.is_empty() {
            let g = self.gate(depth + 1, targets);
            wrap(g, None)
        } else {
            let k = targets[self.rng.below(targets.len())];
            node(EventType::TransferIn, &format!("sub{k}.fta"), vec![])
        }
    }

    fn gate(&mut self, depth: u32, targets: &[usize]) -> Node {
        use EventType::*;
        let kind = [And, Or, Xor, Inhibit, PriorityAnd, Or, And][self.rng.below(7)];
        let mut kids = Vec::new();
        if matches!(kind, Or | Xor) && self.rng.chance(25) {
            let c = self.conds[self.rng.below(self.conds.len())].clone();
            kids.push(node(ConditionAnalysed, &c, vec![]));
            if self.rng.chance(30) {
                kids.push(node(ConditionNotAnalysed, "n0", vec![]));
            }
        } else if self.rng.chance(15) {
            kids.push(node(ConditionNotAnalysed, "n0", vec![]));
        }
        for _ in 0..2 + self.rng.below(3) {
            kids.push(self.child(depth, targets));
        }
        node(kind, "", kids)
    }
}

/// `M [T marker?] gate` as it appears in OpenFTA files.
fn wrap(gate: Node, marker: Option<&str>) -> Node {
    let mut kids = Vec::new();
    if let Some(m) = marker {
        kids.push(node(EventType::TransferOut, m, vec![]));
    }
    kids.push(gate);
    node(EventType::Intermediate, "", kids)
}

fn write_node(n: &Node, out: &mut String) {
    use EventType::*;
    let id = if n.id.is_empty() { "NULL" } else { &n.id };
    let nk = if matches!(n.kind, Basic | TransferIn | TransferOut) {
        0
    } else {
        n.children.len()
    };
    out.push_str(&format!("{} {} {}\n", n.kind.code(), id, nk));
    if n.kind == Intermediate {
        out.push_str("0\n");
    }
    for c in &n.children {
        write_node(c, out);
    }
}

fn tree_text(root: &Node) -> String {
    let mut s = String::from("gen.ped\nS NULL 0\n0\n");
    write_node(root, &mut s);
    s
}

fn temp_dir(name: &str) -> PathBuf {
    let d = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

struct Case {
    dir: PathBuf,
    main: Node,
    subs: Vec<Node>,
    probs: ProbMap,
    unit_time: f64,
}

fn random_case(seed: u64, name: &str) -> Case {
    let mut rng = Prng(seed);
    let nev = 4 + rng.below(6);
    let kinds = [
        EventType::Basic,
        EventType::Basic,
        EventType::Undeveloped,
        EventType::External,
    ];
    let events: Vec<(String, EventType)> = (0..nev)
        .map(|i| (format!("e{i}"), kinds[rng.below(kinds.len())]))
        .collect();
    let conds: Vec<String> = (0..2).map(|i| format!("c{i}")).collect();

    let mut probs = ProbMap::new();
    let mut ped = String::new();
    let mut add = |id: &str, tc: char, rng: &mut Prng| {
        let l = rng.chance(25);
        let v = if l {
            0.01 + 0.4 * rng.unit()
        } else {
            0.02 + 0.5 * rng.unit()
        };
        probs.insert(id.to_string(), (v, l));
        ped.push_str(&format!(
            "{id};;{tc} ;desc;{v:e};{};\n",
            if l { 'L' } else { ' ' }
        ));
    };
    for (id, k) in &events {
        add(id, k.code(), &mut rng);
    }
    for c in &conds {
        add(c, 'C', &mut rng);
    }
    ped.push_str("n0;;N ;not analysed;0.5; ;\n");

    let mut g = Gen {
        rng: &mut rng,
        events,
        conds,
    };
    let nsubs = g.rng.below(3);
    let mut subs: Vec<Option<Node>> = vec![None; nsubs];
    for k in (0..nsubs).rev() {
        let targets: Vec<usize> = (k + 1..nsubs).collect();
        subs[k] = Some(wrap(g.gate(1, &targets), Some(&format!("sub{k}.fta"))));
    }
    let subs: Vec<Node> = subs.into_iter().map(Option::unwrap).collect();
    let targets: Vec<usize> = (0..nsubs).collect();
    let main = wrap(
        g.gate(0, &targets),
        if g.rng.chance(30) {
            Some("main.fta")
        } else {
            None
        },
    );
    let unit_time = 0.5 + 2.5 * rng.unit();

    let dir = temp_dir(name);
    std::fs::write(dir.join("gen.ped"), ped).unwrap();
    std::fs::write(dir.join("main.fta"), tree_text(&main)).unwrap();
    for (k, s) in subs.iter().enumerate() {
        std::fs::write(dir.join(format!("sub{k}.fta")), tree_text(s)).unwrap();
    }
    Case {
        dir,
        main,
        subs,
        probs,
        unit_time,
    }
}

#[test]
fn random_trees_match_truth_table() {
    let mut total_sets = 0;
    let mut nontrivial = 0;
    let mut with_transfer = 0;
    for seed in 0..300u64 {
        let c = random_case(0xF7A4_0000 + seed, &format!("rand_{seed}"));
        let proj = Project::load(c.dir.join("main.fta")).unwrap();
        let res = |n: &str| -> Node {
            let k: usize = n
                .trim_start_matches("sub")
                .trim_end_matches(".fta")
                .parse()
                .unwrap();
            c.subs[k].clone()
        };
        let label = format!("seed {seed} ({})", c.dir.display());
        // only trees whose top event can occur are interesting
        let o = Oracle::new(&c.main, &res);
        if !o.truth.iter().any(|&b| b) {
            continue;
        }
        let n = check(&label, &proj, &c.main, &res, &c.probs, c.unit_time);
        total_sets += n;
        nontrivial += usize::from(n > 1);
        with_transfer += usize::from(!c.subs.is_empty());
        let _ = std::fs::remove_dir_all(&c.dir);
    }
    assert!(
        total_sets > 300 && nontrivial > 150 && with_transfer > 100,
        "{total_sets} {nontrivial} {with_transfer}"
    );
}

#[test]
fn random_trees_monte_carlo_agrees_with_truth_table() {
    let mut compared = 0;
    for seed in 0..12u64 {
        let c = random_case(0xB10C_0000 + seed, &format!("mc_{seed}"));
        let proj = Project::load(c.dir.join("main.fta")).unwrap();
        let res = |n: &str| -> Node {
            let k: usize = n
                .trim_start_matches("sub")
                .trim_end_matches(".fta")
                .parse()
                .unwrap();
            c.subs[k].clone()
        };
        let o = Oracle::new(&c.main, &res);
        let p: Vec<f64> = o
            .ids
            .iter()
            .map(|id| eff(&c.probs, id, c.unit_time))
            .collect();
        let exact = o.prob(&p, |m| o.truth[m]);
        if exact < 0.02 {
            continue;
        }
        let trials = 200_000;
        for method in [Method::FirstFailure, Method::Direct] {
            let opts = MonteCarloOptions {
                trials,
                unit_time: c.unit_time,
                seed: seed + 1,
                method,
            };
            let r = simulate(&proj.tree, &proj.db, &opts).unwrap();
            let tol = 6.0 * (exact * (1.0 - exact) / trials as f64).sqrt() + 1e-4;
            assert!(
                (r.probability - exact).abs() < tol,
                "seed {seed} {method:?}: {} vs {exact}",
                r.probability
            );
            close(
                r.any_component_failure,
                1.0 - p.iter().map(|x| 1.0 - x).product::<f64>(),
                "any component failure",
                "mc",
            );
            // event_failures / trials estimates P(top and event failed)
            for (id, n) in &r.event_failures {
                let i = o.ids.iter().position(|x| x == id).unwrap();
                let joint = o.prob(&p, |m| o.truth[m] && m >> i & 1 == 1);
                let est = r.bas_prob * *n as f64 / trials as f64;
                let tol = 6.0 * (joint * (1.0 - joint) / trials as f64).sqrt() + 1e-4;
                assert!(
                    (est - joint).abs() < tol,
                    "seed {seed} {method:?} event {id}: {est} vs {joint}"
                );
            }
        }
        compared += 1;
        let _ = std::fs::remove_dir_all(&c.dir);
    }
    assert!(compared >= 5, "only {compared} trees compared");
}

// ------------------------------------------------------------ file trees

fn data(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data")
        .join(rel)
}

fn check_file(rel: &str, unit_time: f64) -> usize {
    let path = data(rel);
    let dir = path.parent().unwrap().to_path_buf();
    let proj = Project::load(&path).unwrap();
    let main = parse_fta(&std::fs::read(&path).unwrap()).unwrap().root;
    let res = |name: &str| {
        parse_fta(&std::fs::read(dir.join(name)).unwrap())
            .unwrap()
            .root
    };
    let probs: ProbMap = proj
        .db
        .events
        .iter()
        .map(|(k, e)| (k.clone(), (e.probability, e.lambda == Some('L'))))
        .collect();
    check(rel, &proj, &main, &res, &probs, unit_time)
}

#[test]
fn example_trees_match_truth_table() {
    for (rel, expect_sets) in [
        ("theatre.fta", Some(2)),
        ("lift.fta", None),
        ("top.fta", Some(2)),
        ("middle.fta", Some(2)),
        ("bottom.fta", Some(2)),
        ("threemotor/Three Motor Example (Motor 2 Only).fta", None),
        ("threemotor/T.fta", None),
        ("transtest/transTest.fta", Some(1)),
        ("transtest/transTest1.fta", Some(1)),
        ("transtest/transTest2.fta", Some(1)),
    ] {
        for t in [1.0, 3.7] {
            let n = check_file(rel, t);
            if let Some(e) = expect_sets {
                assert_eq!(n, e, "{rel}: number of cut sets");
            }
        }
    }
}

#[test]
fn theatre_numbers_by_hand() {
    // TOP = AND(MAINS_FAIL, OR(RELAY_FAIL, GEN_FAIL)); GEN_FAIL is a rate event.
    let p = Project::load(data("theatre.fta")).unwrap();
    for t in [1.0, 2.5] {
        let gen = 1.0 - (-0.02f64 * t).exp();
        let (m, r) = (0.03, 0.05);
        let exact = m * (r + gen - r * gen);
        let rep = probability_report(
            &p.tree,
            &p.db,
            &ProbabilityOptions {
                unit_time: t,
                ..Default::default()
            },
        )
        .unwrap();
        close(rep.top, exact, "theatre top", "hand");
        let imp = |id: &str| rep.events.iter().find(|e| e.id == id).unwrap().clone();
        close(
            imp("MAINS_FAIL").contribution,
            m * r + m * gen,
            "MAINS contribution",
            "hand",
        );
        close(
            imp("GEN_FAIL").contribution,
            m * gen,
            "GEN contribution",
            "hand",
        );
        close(
            imp("RELAY_FAIL").importance_pct,
            100.0 * m * r / exact,
            "RELAY importance",
            "hand",
        );
    }
}

// ------------------------------------------------------------ regressions

/// OpenFTA's `load_item` replaces the type letter in the `.fta` file by the type
/// stored in the tree's own primary event database.
#[test]
fn ped_type_overrides_tree_type() {
    let dir = temp_dir("ped_override");
    std::fs::write(
        dir.join("t.fta"),
        "t.ped\nS NULL 0\n0\nM NULL 1\n0\nA NULL 2\nB b 0\nB a 0\n",
    )
    .unwrap();
    // b is a not-analysed condition according to the database, so AND ignores it
    std::fs::write(dir.join("t.ped"), "a;;B ;;0.1; ;\nb;;N ;;0.2; ;\n").unwrap();
    let p = Project::load(dir.join("t.fta")).unwrap();
    let cs = minimal_cut_sets(&p.tree).unwrap();
    assert_eq!(cs, vec![CutSet::from(["a".to_string()])]);

    std::fs::write(
        dir.join("c.fta"),
        "c.ped\nS NULL 0\n0\nM NULL 1\n0\nO NULL 3\nB c 0\nB x 0\nB y 0\n",
    )
    .unwrap();
    // c is an analysed condition according to the database: (x + y) . c
    std::fs::write(
        dir.join("c.ped"),
        "c;;C ;;0.5; ;\nx;;B ;;0.1; ;\ny;;B ;;0.2; ;\n",
    )
    .unwrap();
    let p = Project::load(dir.join("c.fta")).unwrap();
    let cs: BTreeSet<CutSet> = minimal_cut_sets(&p.tree).unwrap().into_iter().collect();
    let want: BTreeSet<CutSet> = [["c", "x"], ["c", "y"]]
        .iter()
        .map(|s| s.iter().map(|e| e.to_string()).collect())
        .collect();
    assert_eq!(cs, want);
    let _ = std::fs::remove_dir_all(&dir);
}

/// `n` levels: alternating intermediate events and OR gates above one basic event.
fn chain(n: usize, leaf: &str) -> String {
    let mut s = String::from("NULL\nS NULL 0\n0\n");
    for i in 0..n - 1 {
        if i % 2 == 0 {
            s.push_str(&format!("M G{i} 1\n1 x\n"));
        } else {
            s.push_str("O NULL 1\n");
        }
    }
    s.push_str(leaf);
    s.push('\n');
    s
}

/// Trees near `MAX_DEPTH` need the 8 MiB stack the binaries run with
/// (test threads get 2 MiB).
#[test]
fn nesting_depth_is_limited() {
    std::thread::Builder::new()
        .stack_size(8 * 1024 * 1024)
        .spawn(nesting_depth_body)
        .unwrap()
        .join()
        .unwrap();
}

fn nesting_depth_body() {
    use ftar_core::model::MAX_DEPTH;
    let t = parse_fta(chain(MAX_DEPTH, "B L 0").as_bytes()).unwrap();
    assert_eq!(t.root.depth(), MAX_DEPTH);
    let e = parse_fta(chain(MAX_DEPTH + 1, "B L 0").as_bytes()).unwrap_err();
    assert!(e.to_string().contains("levels deep"), "{e}");
    // far beyond the limit: still an error, not a stack overflow
    let e = parse_fta(chain(200_001, "B L 0").as_bytes()).unwrap_err();
    assert!(e.to_string().contains("levels deep"), "{e}");

    // Two files each within the limit, joined by a transfer-in, exceed it.
    let dir = temp_dir("depth_transfer");
    std::fs::write(dir.join("a.fta"), chain(MAX_DEPTH - 1, "I b.fta 0")).unwrap();
    std::fs::write(dir.join("b.fta"), chain(MAX_DEPTH - 1, "B L 0")).unwrap();
    let e = Project::load(dir.join("a.fta")).unwrap_err();
    assert!(e.to_string().contains("levels deep"), "{e}");

    // Edits cannot build a deeper tree either. The OR gate at level
    // MAX_DEPTH - 2 sits at a path of MAX_DEPTH - 3 zeros.
    use ftar_core::edit::{append_child, new_node, EditError};
    let mut t = parse_fta(chain(MAX_DEPTH, "B L 0").as_bytes()).unwrap();
    let gate = vec![0; MAX_DEPTH - 3];
    assert_eq!(
        ftar_core::edit::node_at(&t, &gate).unwrap().kind,
        EventType::Or
    );
    let subtree = |levels: usize| {
        // M -> B (2 levels) or M -> O -> B (3 levels)
        let mut m = new_node(EventType::Intermediate, "N");
        let leaf = new_node(EventType::Basic, "K");
        if levels == 2 {
            m.children.push(leaf);
        } else {
            let mut o = new_node(EventType::Or, "");
            o.children.push(leaf);
            m.children.push(o);
        }
        m
    };
    assert_eq!(
        append_child(&mut t.clone(), &gate, subtree(3)),
        Err(EditError::TooDeep)
    );
    assert!(append_child(&mut t, &gate, subtree(2)).is_ok());
    assert_eq!(t.root.depth(), MAX_DEPTH);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn confined_load_ignores_files_outside_the_root() {
    use ftar_core::project::LoadOptions;
    let dir = temp_dir("confined_load");
    let root = dir.join("root");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(dir.join("out.ped"), "L;;B ;;0.9; ;\n").unwrap();
    std::fs::write(
        root.join("t.fta"),
        chain(3, "B L 0").replace("NULL\nS", "../out.ped\nS"),
    )
    .unwrap();
    let opts = LoadOptions::confined(&root).unwrap();
    assert!(Project::load_with(root.join("t.fta"), &opts)
        .unwrap()
        .db
        .events
        .is_empty());
    // unconfined, the database is found as before
    assert_eq!(
        Project::load(root.join("t.fta")).unwrap().db.events.len(),
        1
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Neighbouring seeds used to collapse onto the same generator state (`seed | 1`).
#[test]
fn monte_carlo_neighbouring_seeds_differ() {
    let p = Project::load(data("theatre.fta")).unwrap();
    let run = |seed| {
        let r = simulate(
            &p.tree,
            &p.db,
            &MonteCarloOptions {
                trials: 200_000,
                seed,
                ..Default::default()
            },
        )
        .unwrap();
        (r.system_failures, r.event_failures, r.cut_set_hits)
    };
    for (a, b) in [(0, 1), (2, 3), (10, 11)] {
        assert_ne!(run(a), run(b), "seeds {a} and {b} give identical runs");
    }
    assert_eq!(run(7), run(7));
}
