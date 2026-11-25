#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! PFTA compatibility: reading and writing PFTA's files, and the
//! time-dependent quantification (`ftar_core::quant`) checked against an
//! independent oracle: brute force over every state of the events, with the
//! exact failure intensity `ω = Σ ω_e (Q(e failed) − Q(e working))`.

use ftar_core::project::Document;
use ftar_core::quant::{analyse, analyse_document, tsv, QuantOptions, QuantReport};
use ftar_core::text::{
    parse_text, parse_text_ast, pfta_obstacles, write_text, Ast, GateType, Model, Param,
};

fn report(src: &str) -> QuantReport {
    analyse(
        &parse_text_ast(src.as_bytes()).unwrap(),
        &QuantOptions::default(),
    )
    .unwrap()
}

fn gate<'a>(r: &'a QuantReport, id: &str) -> &'a ftar_core::quant::GateResult {
    r.gates.iter().find(|g| g.id == id).unwrap()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-12 * a.abs().max(b.abs()).max(1e-300)
}

// ------------------------------------------------------------- worked examples

const TOAST: &str = "\
- times: nan
- time_unit: yr

Gate: FB
- label: Floor buttered
- type: OR
- inputs: BF, TFBSD

Event: BF
- model_type: Fixed
- probability: 0
- intensity: 0.1

Gate: TFBSD
- type: AND
- inputs: TF, TB, BSD

Event: TF
- model_type: Fixed
- probability: 0
- intensity: 0.2

Event: TB
- model_type: Fixed
- probability: 0.75
- intensity: 0

Event: BSD
- model_type: Fixed
- probability: 0.9
- intensity: 0
";

#[test]
fn fixed_intensities_add_up() {
    let r = report(TOAST);
    let fb = gate(&r, "FB");
    assert!(fb.is_top_gate);
    assert_eq!(fb.series.q, [0.0]);
    assert!(close(fb.series.w[0], 0.1 + 0.2 * 0.75 * 0.9));
    assert!(close(fb.series.rate[0], fb.series.w[0]));
    assert!(!gate(&r, "TFBSD").is_top_gate);
    assert!(close(gate(&r, "TFBSD").series.w[0], 0.135));
    // The cut sets of the top gate, in order of size.
    let names: Vec<String> = fb.cut_sets.iter().map(|c| c.events.join(".")).collect();
    assert_eq!(names, ["BF", "TF.TB.BSD"]);
}

#[test]
fn pfta_readme_example() {
    // A (ω 0.9) AND B (q 0.7): ω 0.63; then OR C (ω 1e-4): rate 0.6301.
    let src = "- times: nan\n\nGate: T\n- type: OR\n- inputs: AB, C\n\nGate: AB\n- type: AND\n- inputs: A, B\n\n\
Event: A\n- model_type: Fixed\n- probability: 0\n- intensity: 0.9\n\n\
Event: B\n- model_type: Fixed\n- probability: 0.7\n- intensity: 0\n\n\
Event: C\n- model_type: Fixed\n- probability: 0\n- intensity: 1e-4\n";
    let r = report(src);
    assert!(close(gate(&r, "AB").series.rate[0], 0.63));
    assert!(close(gate(&r, "T").series.rate[0], 0.6301));
}

#[test]
fn constant_rates_over_time() {
    let src = "- times: 0, 10, 100\n- time_unit: h\n\nGate: T\n- type: AND\n- inputs: A, B\n\n\
Model: M\n- model_type: ConstantRate\n- mean_failure_time: 50\n- mean_repair_time: inf\n\n\
Event: A\n- model: M\n\n\
Event: B\n- model_type: ConstantRate\n- failure_rate: 0.01\n- repair_rate: 0.09\n";
    let r = report(src);
    assert_eq!(r.times, [0.0, 10.0, 100.0]);
    let t = gate(&r, "T");
    for (i, time) in [0.0f64, 10.0, 100.0].into_iter().enumerate() {
        let qa = 1.0 - (-time / 50.0).exp();
        let wa = (-time / 50.0).exp() / 50.0;
        let qb = 0.01 / 0.1 * (1.0 - (-0.1 * time).exp());
        let wb = 0.01 * (1.0 - qb);
        assert!(close(t.series.q[i], qa * qb), "q at {time}");
        let w = wa * qb + qa * wb;
        assert!(
            (t.series.w[i] - w).abs() <= 1e-15 + 1e-12 * w,
            "ω at {time}"
        );
    }
    assert!(r.models[0].is_used);
}

#[test]
fn true_and_false_events_simplify() {
    let src = "- times: 1\n\nGate: T\n- type: OR\n- inputs: G, F\n\nGate: G\n- type: AND\n- inputs: A, Y\n\n\
Event: A\n- model_type: Fixed\n- probability: 0.3\n- intensity: 0.1\n\n\
Event: Y\n- appearance: House\n- model_type: True\n\n\
Event: F\n- model_type: False\n";
    let r = report(src);
    let t = gate(&r, "T");
    assert!(close(t.series.q[0], 0.3));
    assert_eq!(t.cut_sets.len(), 1);
    assert_eq!(t.cut_sets[0].events, ["A"]);
    // An OR with a True input is always failed.
    let src = "- times: 1\n\nGate: T\n- type: OR\n- inputs: A, Y\n\n\
Event: A\n- model_type: Fixed\n- probability: 0.3\n- intensity: 0.1\n\n\
Event: Y\n- model_type: True\n";
    let t = report(src);
    let t = gate(&t, "T");
    assert_eq!((t.series.q[0], t.series.w[0]), (1.0, 0.0));
    assert!(t.cut_sets[0].events.is_empty());
    let table = tsv::cut_sets(&report(src), 0).unwrap();
    assert!(
        table
            .lines()
            .nth(1)
            .unwrap()
            .starts_with("True\t0\t1.0\t0\t1.0\t0.0"),
        "{table}"
    );
}

#[test]
fn vote_gates() {
    let src = "- times: 1\n\nGate: T\n- type: VOTE(2)\n- inputs: A, B, C\n\n\
Model: M\n- model_type: Fixed\n- probability: 0.1\n- intensity: 0.5\n\n\
Event: A\n- model: M\n\nEvent: B\n- model: M\n\nEvent: C\n- model: M\n";
    let r = report(src);
    let t = gate(&r, "T");
    let p: f64 = 0.1;
    assert!(close(t.series.q[0], 3.0 * p * p * (1.0 - p) + p * p * p));
    // Each event is critical when exactly one other has failed.
    assert!(close(t.series.w[0], 3.0 * 0.5 * 2.0 * p * (1.0 - p)));
    assert_eq!(t.cut_sets.len(), 3);
    // The tree holds the expansion, the file keeps the VOTE.
    let d = parse_text(src.as_bytes()).unwrap();
    let text = write_text(&d.tree, &d.db, &d.extras).unwrap();
    assert!(
        text.contains("- type: VOTE(2)\n- inputs: A, B, C\n"),
        "{text}"
    );
    assert!(text.contains("- model: M\n"), "{text}");
    // Editing the expansion turns it into ordinary gates.
    let mut tree = d.tree.clone();
    tree.root.children[0].children.pop();
    let text = write_text(&tree, &d.db, &d.extras).unwrap();
    assert!(!text.contains("VOTE"), "{text}");
}

// ------------------------------------------------------------- oracle

/// A tiny generator for the random trees.
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    fn unit(&mut self) -> f64 {
        self.next() as f64 / (1u64 << 31) as f64
    }
}

/// A random coherent tree over `n` events, written as a file.
fn random_tree(rng: &mut Lcg, n: usize) -> String {
    let mut src = String::from("- times: 1\n");
    for e in 0..n {
        let q = 0.5 * rng.unit();
        let w = rng.unit();
        src +=
            &format!("\nEvent: E{e}\n- model_type: Fixed\n- probability: {q}\n- intensity: {w}\n");
    }
    // Gates over events and lower gates; the last one is the top.
    let gates = 2 + rng.below(4) as usize;
    for g in 0..gates {
        let pool = n + g;
        let k = 2 + rng.below(3) as usize;
        let mut ins: Vec<String> = Vec::new();
        while ins.len() < k.min(pool) {
            let i = rng.below(pool as u64) as usize;
            let name = if i < n {
                format!("E{i}")
            } else {
                format!("G{}", i - n)
            };
            if !ins.contains(&name) {
                ins.push(name);
            }
        }
        // The top uses every lower gate, so all are under it.
        if g + 1 == gates {
            for h in 0..g {
                let name = format!("G{h}");
                if !ins.contains(&name) {
                    ins.push(name);
                }
            }
        }
        let t = match rng.below(3) {
            0 => "AND".to_string(),
            1 => "OR".to_string(),
            _ => format!("VOTE({})", 1 + rng.below(ins.len() as u64)),
        };
        src += &format!("\nGate: G{g}\n- type: {t}\n- inputs: {}\n", ins.join(", "));
    }
    src
}

/// Is gate `id` failed when the events in `state` (bit e) have failed?
fn failed(ast: &Ast, id: &str, state: u32) -> bool {
    if let Some(i) = ast.events.iter().position(|e| e.id == id) {
        return state & (1 << i) != 0;
    }
    let g = ast.gates.iter().find(|g| g.id == id).unwrap();
    let n = g.inputs.iter().filter(|i| failed(ast, i, state)).count();
    match g.gate_type {
        GateType::And => n == g.inputs.len(),
        GateType::Or => n > 0,
        GateType::Vote(k) => n >= k,
        t => panic!("{t:?}"),
    }
}

fn fixed(ast: &Ast) -> Vec<(f64, f64)> {
    ast.events
        .iter()
        .map(|e| match &e.model {
            Some(ftar_core::text::EventModel::Own(Model::Fixed {
                probability: Param::Value(q),
                intensity: Param::Value(w),
            })) => (*q, *w),
            m => panic!("{m:?}"),
        })
        .collect()
}

/// Exact probability that `id` has failed, with event `pin` fixed (if any).
fn exact_q(ast: &Ast, id: &str, qw: &[(f64, f64)], pin: Option<(usize, bool)>) -> f64 {
    let n = qw.len();
    let mut total = 0.0;
    for state in 0..(1u32 << n) {
        let mut p = 1.0;
        for (e, (q, _)) in qw.iter().enumerate() {
            let on = state & (1 << e) != 0;
            p *= match pin {
                Some((i, v)) if i == e => f64::from(u8::from(on == v)),
                _ if on => *q,
                _ => 1.0 - q,
            };
        }
        if p > 0.0 && failed(ast, id, state) {
            total += p;
        }
    }
    total
}

#[test]
fn random_trees_match_the_exact_values() {
    let mut rng = Lcg(7);
    for case in 0..60 {
        let n = 2 + rng.below(6) as usize;
        let src = random_tree(&mut rng, n);
        let ast = parse_text_ast(src.as_bytes()).unwrap();
        let r = analyse(&ast, &QuantOptions::default()).unwrap();
        let qw = fixed(&ast);
        for g in &r.gates {
            let q = exact_q(&ast, &g.id, &qw, None);
            let w: f64 = (0..n)
                .map(|e| {
                    let b = exact_q(&ast, &g.id, &qw, Some((e, true)))
                        - exact_q(&ast, &g.id, &qw, Some((e, false)));
                    qw[e].1 * b
                })
                .sum();
            let tol = |x: f64| 1e-10 * x.abs().max(1e-12);
            assert!(
                (g.series.q[0] - q).abs() <= tol(q),
                "case {case} gate {}: q {} vs {q}\n{src}",
                g.id,
                g.series.q[0]
            );
            assert!(
                (g.series.w[0] - w).abs() <= tol(w),
                "case {case} gate {}: ω {} vs {w}\n{src}",
                g.id,
                g.series.w[0]
            );
        }
    }
}

#[test]
fn truncation_order_one_is_the_rare_event_approximation() {
    let src = "- times: 1\n- computational_order: 1\n\nGate: T\n- type: OR\n- inputs: A, B\n\n\
Event: A\n- model_type: Fixed\n- probability: 0.1\n- intensity: 1\n\n\
Event: B\n- model_type: Fixed\n- probability: 0.2\n- intensity: 2\n";
    let t = report(src);
    let t = gate(&t, "T");
    assert!(close(t.series.q[0], 0.3));
    assert!(close(t.series.w[0], 3.0));
    let all = analyse(
        &parse_text_ast(src.as_bytes()).unwrap(),
        &QuantOptions {
            computational_order: Some(2),
            ..QuantOptions::default()
        },
    )
    .unwrap();
    assert!(close(gate(&all, "T").series.q[0], 0.28));
}

#[test]
fn a_huge_expansion_is_refused_not_run() {
    let mut src = String::from("- times: 1\n\nGate: T\n- type: OR\n- inputs: ");
    let ids: Vec<String> = (0..40).map(|i| format!("E{i}")).collect();
    src += &ids.join(", ");
    src.push('\n');
    for id in &ids {
        src += &format!("\nEvent: {id}\n- probability: 0.01\n");
    }
    let e = analyse(
        &parse_text_ast(src.as_bytes()).unwrap(),
        &QuantOptions {
            budget: 1_000_000,
            ..QuantOptions::default()
        },
    )
    .unwrap_err();
    assert!(e.to_string().contains("computational_order"), "{e}");
}

// ------------------------------------------------------------- sampling

#[test]
fn sampling_is_reproducible_and_has_the_right_mean() {
    let src =
        "- times: 1, 2\n- seed: abc\n- sample_size: 4000\n\nGate: T\n- type: OR\n- inputs: A\n\n\
Event: A\n- model_type: Fixed\n- probability: uniform(lower=0.1, upper=0.3)\n- intensity: 0\n";
    let r = report(src);
    let t = gate(&r, "T");
    assert_eq!(t.series.q.len(), 8000);
    let mean = r.expected(&t.series.q);
    assert_eq!(mean.len(), 2);
    for m in &mean {
        assert!((m - 0.2).abs() < 0.005, "{m}");
    }
    // Each time draws its own samples.
    assert_ne!(t.series.q[..4000], t.series.q[4000..]);
    let q = r.quantile(&t.series.q, 0.5);
    assert!((q[0] - 0.2).abs() < 0.01);
    assert_eq!(report(src), r);
    let other = analyse(
        &parse_text_ast(src.as_bytes()).unwrap(),
        &QuantOptions {
            seed: Some("xyz".into()),
            ..QuantOptions::default()
        },
    )
    .unwrap();
    assert_ne!(gate(&other, "T").series.q, t.series.q);
}

#[test]
fn shared_models_are_sampled_per_event() {
    let src = "- times: 1\n- sample_size: 50\n\nGate: T\n- type: AND\n- inputs: A, B\n\n\
Model: M\n- model_type: Fixed\n- probability: uniform(lower=0, upper=1)\n- intensity: 0\n\n\
Event: A\n- model: M\n\nEvent: B\n- model: M\n";
    let r = report(src);
    assert_ne!(r.events[0].series.q, r.events[1].series.q);
}

#[test]
fn out_of_range_samples_are_errors() {
    let src = "- times: 1\n- sample_size: 1000\n\nGate: T\n- type: OR\n- inputs: A\n\n\
Event: A\n- model_type: Fixed\n- probability: normal(mu=0.5, sigma=1)\n- intensity: 0\n";
    let e = analyse(
        &parse_text_ast(src.as_bytes()).unwrap(),
        &QuantOptions::default(),
    )
    .unwrap_err();
    assert!(e.to_string().contains("out of range"), "{e}");
}

// ------------------------------------------------------------- files

const PFTA_FILE: &str = "\
# A file as PFTA writes them.
- times: 0, 1000, 2000
- time_unit: h
- seed: Moo
- sample_size: 3
- computational_order: 4
- computational_tolerance: 1e-9
- significant_figures: 4
- scientific_exponent: 2

Model: PUMP
- label: Pump
- comment: From the data book
- model_type: ConstantRate
- failure_rate: lognormal(mu=-9, sigma=0.5)
- mean_repair_time: 24

Gate: TOP
- label: No flow
- comment: Top event
- is_paged: True
- type: VOTE(2)
- inputs: P1, P2, P3

Gate: OTHER
- type: NULL
- inputs: P1

Event: P1
- model: PUMP

Event: P2
- model: PUMP

Event: P3
- label: Spare pump
- comment: Usually idle
- appearance: Undeveloped
- model_type: Fixed
- probability: triangular(lower=0.001, upper=0.01, mode=0.002)
- intensity: 0

Event: UNUSED
- model_type: True
";

#[test]
fn pfta_files_round_trip_and_stay_pfta_files() {
    let d = parse_text(PFTA_FILE.as_bytes()).unwrap();
    // The tree is the first top gate; the other is kept.
    assert_eq!(d.tree.root.id, "TOP");
    assert_eq!(d.extras.detached.len(), 1);
    let text = write_text(&d.tree, &d.db, &d.extras).unwrap();
    let ast = parse_text_ast(text.as_bytes()).unwrap();
    assert_eq!(pfta_obstacles(&ast), Vec::<String>::new(), "{text}");
}

trait WithoutLines {
    fn clone_without_lines(&self) -> Ast;
}

impl WithoutLines for Ast {
    /// The same paragraphs with line numbers zeroed and in written order.
    fn clone_without_lines(&self) -> Ast {
        let mut a = self.clone();
        a.models.iter_mut().for_each(|m| m.line = 0);
        a.gates.iter_mut().for_each(|g| g.line = 0);
        a.events.iter_mut().for_each(|e| e.line = 0);
        a
    }
}

#[test]
fn pfta_files_keep_everything() {
    let d = parse_text(PFTA_FILE.as_bytes()).unwrap();
    let text = write_text(&d.tree, &d.db, &d.extras).unwrap();
    let mut got = parse_text_ast(text.as_bytes())
        .unwrap()
        .clone_without_lines();
    let mut want = parse_text_ast(PFTA_FILE.as_bytes())
        .unwrap()
        .clone_without_lines();
    // Order of paragraphs may change: compare as sets.
    for a in [&mut got, &mut want] {
        a.gates.sort_by(|x, y| x.id.cmp(&y.id));
        a.events.sort_by(|x, y| x.id.cmp(&y.id));
    }
    assert_eq!(got, want, "{text}");
    // Writing again changes nothing.
    let again = parse_text(text.as_bytes()).unwrap();
    assert_eq!(
        write_text(&again.tree, &again.db, &again.extras).unwrap(),
        text
    );
}

#[test]
fn openfta_trees_say_why_they_are_not_pfta_files() {
    let doc = Document::load(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/data/theatre.fta"
    ))
    .unwrap();
    let text = write_text(&doc.tree, &doc.db, &doc.extras).unwrap();
    let ast = parse_text_ast(text.as_bytes()).unwrap();
    let why = pfta_obstacles(&ast);
    assert!(why.iter().any(|w| w.contains("note")), "{why:?}");
    assert!(why.iter().any(|w| w.contains("OpenFTA ids")), "{why:?}");
    // Its events are written as PFTA models.
    assert!(
        text.contains("- model_type: ConstantRate\n- failure_rate: 0.02\n- repair_rate: 0\n"),
        "{text}"
    );
    assert!(
        text.contains("- model_type: Fixed\n- probability: 0.03\n- intensity: 0\n"),
        "{text}"
    );
}

#[test]
fn an_edited_probability_replaces_the_model() {
    let d = parse_text(PFTA_FILE.as_bytes()).unwrap();
    let mut db = d.db.clone();
    // P3 has a distribution: no database entry. Give it a probability.
    assert!(!db.events.contains_key("P3"));
    db.events.insert(
        "P3".into(),
        ftar_core::PrimaryEvent {
            id: "P3".into(),
            group: String::new(),
            kind: ftar_core::EventType::Undeveloped,
            modifier: None,
            description: "Spare pump".into(),
            probability: 0.5,
            lambda: None,
        },
    );
    let text = write_text(&d.tree, &db, &d.extras).unwrap();
    assert!(text.contains("- probability: 0.5\n"), "{text}");
    assert!(!text.contains("triangular"), "{text}");
    assert!(text.contains("- comment: Usually idle\n"), "{text}");
}

#[test]
fn documents_are_quantified_with_their_models() {
    let dir = std::env::temp_dir().join(format!("ftar_pfta_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("pumps.ftt");
    std::fs::write(&path, PFTA_FILE).unwrap();
    let doc = Document::load(&path).unwrap();
    let r = analyse_document(&doc, &QuantOptions::default()).unwrap();
    assert_eq!(r.times, [0.0, 1000.0, 2000.0]);
    assert_eq!(r.sample_size, 3);
    let top = gate(&r, "TOP");
    assert!(top.is_top_gate && top.is_paged);
    assert_eq!(top.series.q.len(), 9);
    assert!(top.series.q[..3].iter().all(|q| *q == 0.0));
    assert!(top.series.q[3..].iter().all(|q| *q > 0.0 && *q < 0.01));
    let _ = std::fs::remove_dir_all(dir);
    // The tables have PFTA's columns.
    let g = tsv::gates(&r);
    assert!(g.starts_with("id\tlabel\tis_top_gate\tis_paged\ttype\tinputs\ttime\tsample\t"));
    assert!(
        g.contains("TOP\tNo flow\tTrue\tTrue\tVOTE\tP1,P2,P3\t0.0\t0\t0.0\t"),
        "{g}"
    );
    let m = tsv::models(&r);
    assert_eq!(m, "id\tlabel\tis_used\nPUMP\tPump\tTrue\n");
    let e = tsv::events(&r);
    assert!(e.contains("UNUSED\t\tFalse\t0.0\t0\t1.0\t0.0\t"), "{e}");
}

// ------------------------------------------------------------- regressions

/// Written text reads back to text that writes the same.
fn stable(src: &str) -> String {
    let d = parse_text(src.as_bytes()).unwrap();
    let text = write_text(&d.tree, &d.db, &d.extras).unwrap();
    let again = parse_text(text.as_bytes()).unwrap();
    assert_eq!((&again.tree, &again.db), (&d.tree, &d.db), "{text}");
    assert_eq!(
        write_text(&again.tree, &again.db, &again.extras).unwrap(),
        text
    );
    text
}

#[test]
fn a_transfer_in_cannot_have_a_model() {
    // ClusterFuzzLite on #141: the probability was dropped on the first
    // write, and the event itself on the second.
    let e =
        parse_text(b"# Ate\nEvent: ../theatre.fta\n- type: transfer_in\n- probability: 0.001\n")
            .unwrap_err();
    assert!(e.to_string().contains("transfer-in has no model"), "{e}");
}

#[test]
fn models_the_database_cannot_hold_are_kept() {
    // A dormant event has no database entry; its model stays in the file.
    let text = stable(
        "- times: 1\n\nGate: T\n- type: OR\n- inputs: D\n\n\
Event: D\n- type: dormant\n- model_type: Fixed\n- probability: 0.2\n- intensity: 0\n",
    );
    assert!(
        text.contains("- type: dormant\n- model_type: Fixed\n- probability: 0.2\n"),
        "{text}"
    );
}

#[test]
fn bare_unused_events_are_kept() {
    let text =
        stable("- times: 1\n\nGate: T\n- type: OR\n- inputs: A\n\nEvent: A\n\nEvent: LONELY\n");
    assert!(text.contains("Event: LONELY\n"), "{text}");
}
