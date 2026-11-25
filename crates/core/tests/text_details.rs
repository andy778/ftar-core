#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! Details of the text format: every gate type and distribution, the
//! parameter checks, what makes a file a PFTA file, and how values are
//! written. (Each pins down behaviour a mutation test found unchecked.)

use ftar_core::text::{
    is_pfta_id, parse_text, parse_text_ast, pfta_obstacles, write_text, Distribution, EventModel,
    GateType, Model, Param,
};
use ftar_core::EventType;

fn gate_type(src: &str) -> GateType {
    parse_text_ast(
        format!("Gate: T\n- type: {src}\n- inputs: A, B\n\nEvent: A\n\nEvent: B\n").as_bytes(),
    )
    .unwrap()
    .gates[0]
        .gate_type
}

#[test]
fn every_gate_type_reads_and_round_trips() {
    for (name, t, kind) in [
        ("AND", GateType::And, Some(EventType::And)),
        ("OR", GateType::Or, Some(EventType::Or)),
        ("xor", GateType::Xor, Some(EventType::Xor)),
        ("INHIBIT", GateType::Inhibit, Some(EventType::Inhibit)),
        (
            "Priority_And",
            GateType::PriorityAnd,
            Some(EventType::PriorityAnd),
        ),
        ("VOTE( 2 )", GateType::Vote(2), None),
    ] {
        assert_eq!(gate_type(name), t, "{name}");
        assert_eq!(t.openfta_kind(), kind, "{name}");
        if let Some(k) = kind {
            assert_eq!(GateType::from_openfta(k), Some(t), "{name}");
            // The tree has that gate, and writing keeps its type.
            let src = format!("Gate: T\n- type: {name}\n- inputs: A, B\n\nEvent: A\n\nEvent: B\n");
            let d = parse_text(src.as_bytes()).unwrap();
            assert_eq!(d.tree.root.children[0].kind, k, "{name}");
            let text = write_text(&d.tree, &d.db, &d.extras).unwrap();
            assert!(text.contains(&format!("- type: {}\n", t.name())), "{text}");
        }
    }
    assert_eq!(GateType::from_openfta(EventType::Basic), None);
    for t in [
        GateType::Null,
        GateType::And,
        GateType::Or,
        GateType::Vote(3),
    ] {
        assert!(t.is_pfta(), "{t:?}");
    }
    for t in [GateType::Xor, GateType::Inhibit, GateType::PriorityAnd] {
        assert!(!t.is_pfta(), "{t:?}");
    }
}

#[test]
fn pfta_ids() {
    for id in ["a", "A-b_9", "123"] {
        assert!(is_pfta_id(id), "{id}");
    }
    for id in ["", "a.b", "a b", "ä", "a/b", "x:"] {
        assert!(!is_pfta_id(id), "{id}");
    }
}

fn event_param(value: &str) -> Result<Param, String> {
    let src = format!("Event: E\n- model_type: Fixed\n- probability: {value}\n- intensity: 0\n");
    let ast = parse_text_ast(src.as_bytes()).map_err(|e| e.to_string())?;
    match &ast.events[0].model {
        Some(EventModel::Own(Model::Fixed { probability, .. })) => Ok(*probability),
        m => panic!("{m:?}"),
    }
}

#[test]
fn every_distribution_reads_and_writes() {
    use Distribution::*;
    let cases = [
        (
            "beta(alpha=2, beta=5)",
            Beta {
                alpha: 2.0,
                beta: 5.0,
            },
        ),
        (
            "gamma(alpha=3, lambda=2)",
            Gamma {
                alpha: 3.0,
                lambda: 2.0,
            },
        ),
        (
            "lognormal(mu=-2, sigma=0.5)",
            LogNormal {
                mu: -2.0,
                sigma: 0.5,
            },
        ),
        (
            "loguniform(lower=0.001, upper=0.1)",
            LogUniform {
                lower: 0.001,
                upper: 0.1,
            },
        ),
        (
            "normal(mu=0.5, sigma=0.1)",
            Normal {
                mu: 0.5,
                sigma: 0.1,
            },
        ),
        (
            "triangular(lower=0.1, upper=0.3, mode=0.2)",
            Triangular {
                lower: 0.1,
                upper: 0.3,
                mode: 0.2,
            },
        ),
        (
            "uniform(lower=0.1, upper=0.2)",
            Uniform {
                lower: 0.1,
                upper: 0.2,
            },
        ),
        // Any order of the parameters, spaces anywhere.
        (
            " uniform ( upper = 0.2 , lower = 0.1 ) ",
            Uniform {
                lower: 0.1,
                upper: 0.2,
            },
        ),
    ];
    for (text, want) in cases {
        assert_eq!(event_param(text).unwrap(), Param::Dist(want), "{text}");
        // Written back in the same form.
        let src = format!(
            "- times: 1\n\nEvent: E\n- model_type: Fixed\n- probability: {text}\n- intensity: 0\n"
        );
        let d = parse_text(src.as_bytes()).unwrap();
        let out = write_text(&d.tree, &d.db, &d.extras).unwrap();
        let again = parse_text_ast(out.as_bytes()).unwrap();
        assert_eq!(
            again.events[0].model,
            Some(EventModel::Own(Model::Fixed {
                probability: Param::Dist(want),
                intensity: Param::Value(0.0)
            })),
            "{out}"
        );
    }
}

#[test]
fn distribution_parameters_are_checked_as_pfta_does() {
    for bad in [
        "beta(alpha=0, beta=1)",
        "beta(alpha=-1, beta=1)",
        "beta(alpha=1, beta=inf)",
        "gamma(alpha=0, lambda=1)",
        "gamma(alpha=1, lambda=-2)",
        "gamma(alpha=nan, lambda=1)",
        "loguniform(lower=0, upper=1)",
        "loguniform(lower=-1, upper=1)",
        "loguniform(lower=0.1, upper=0)",
        "loguniform(lower=0.1, upper=-1)",
    ] {
        assert!(event_param(bad).is_err(), "{bad}");
    }
    for ok in [
        "beta(alpha=0.5, beta=1e-3)",
        "gamma(alpha=0.1, lambda=10)",
        "loguniform(lower=1e-9, upper=0.5)",
        // PFTA checks nothing else.
        "normal(mu=-5, sigma=-1)",
        "uniform(lower=2, upper=1)",
    ] {
        assert!(event_param(ok).is_ok(), "{ok}");
    }
}

#[test]
fn infinities_are_written_as_pfta_reads_them() {
    let src = "- times: 1\n\nEvent: E\n- model_type: Fixed\n- probability: 0\n- intensity: inf\n\n\
Event: F\n- model_type: Fixed\n- probability: normal(mu=-inf, sigma=1)\n- intensity: 0\n";
    let d = parse_text(src.as_bytes()).unwrap();
    let text = write_text(&d.tree, &d.db, &d.extras).unwrap();
    assert!(text.contains("- intensity: inf\n"), "{text}");
    assert!(text.contains("normal(mu=-inf, sigma=1)"), "{text}");
}

#[test]
fn untyped_gates_write_null_only_with_one_input() {
    let src = "- times: 1\n\nGate: T\n- inputs: U, A\n\nGate: U\n- inputs: B\n\n\
Event: A\n- probability: 0.1\n\nEvent: B\n- probability: 0.2\n";
    let d = parse_text(src.as_bytes()).unwrap();
    let text = write_text(&d.tree, &d.db, &d.extras).unwrap();
    let t = text
        .split("Gate: T\n")
        .nth(1)
        .unwrap()
        .split("\n\n")
        .next()
        .unwrap();
    assert!(!t.contains("- type:"), "{t}");
    let u = text
        .split("Gate: U\n")
        .nth(1)
        .unwrap()
        .split("\n\n")
        .next()
        .unwrap();
    assert!(u.contains("- type: NULL"), "{u}");
    assert!(parse_text(text.as_bytes()).is_ok(), "{text}");
}

#[test]
fn every_obstacle_to_a_pfta_file_is_named() {
    let base = "- times: 1\n\nGate: T\n- type: OR\n- inputs: A, B\n\n\
Event: A\n- model_type: Fixed\n- probability: 0.1\n- intensity: 0\n\n\
Event: B\n- model_type: Fixed\n- probability: 0.1\n- intensity: 0\n";
    let why = |src: &str| pfta_obstacles(&parse_text_ast(src.as_bytes()).unwrap());
    assert!(why(base).is_empty(), "{:?}", why(base));
    let cases: &[(String, &str)] = &[
        (
            base.replace("- times: 1\n", "- times: 1\n- note: n\n"),
            "note",
        ),
        (
            base.replace("- times: 1\n", "- times: 1\n- transfer_out: X\n"),
            "transfer-out",
        ),
        (base.replace("- times: 1\n", ""), "no `times`"),
        (
            base.replace("Event: B", "Event: B.c")
                .replace("A, B", "A, B.c"),
            "characters",
        ),
        (base.replace("- type: OR", "- type: XOR"), "is XOR"),
        (
            base.replace("- type: OR\n- inputs: A, B", "- inputs: A, B"),
            "has no type",
        ),
        (
            base.replace("- inputs: A, B\n", "- inputs: A, B\n- gate_id: G\n"),
            "OpenFTA ids",
        ),
        (
            base.replace("- inputs: A, B\n", "- inputs: A, B\n- id: TT\n"),
            "OpenFTA ids",
        ),
        (
            base.replace("- type: OR\n- inputs: A, B\n", "- type: OR\n- inputs: \n")
                + "\nGate: U\n- type: OR\n- inputs: A, B\n",
            "no inputs",
        ),
        (
            base.replace("Event: A\n", "Event: A\n- type: condition\n"),
            "OpenFTA type",
        ),
        (
            base.replace("Event: A\n", "Event: A\n- dormant: True\n"),
            "OpenFTA data",
        ),
        (
            base.replace("Event: A\n", "Event: A\n- group: g\n"),
            "OpenFTA data",
        ),
        (
            base.replace(
                "Event: B\n- model_type: Fixed\n- probability: 0.1\n- intensity: 0\n",
                "Event: B\n",
            ),
            "no model",
        ),
    ];
    for (src, want) in cases {
        let w = why(src);
        assert!(w.iter().any(|x| x.contains(want)), "{want}: {w:?}\n{src}");
    }
}

#[test]
fn extras_hold_only_what_the_tree_cannot() {
    let src = "- times: 1\n- note: n\n- transfer_out: X\n\n\
Gate: T\n- type: OR\n- inputs: P, Q, U\n\n\
Gate: P\n- is_paged: True\n- type: AND\n- inputs: A, B\n\n\
Gate: Q\n- comment: c\n- type: AND\n- inputs: A, C\n\n\
Gate: U\n- unnamed: True\n- comment: lost\n- type: AND\n- inputs: B, C\n\n\
Event: A\n- probability: 0.1\n\nEvent: B\n- probability: 0.2\n\nEvent: C\n- probability: 0.3\n";
    let d = parse_text(src.as_bytes()).unwrap();
    // The note and transfer-out are in the tree, not the extras.
    assert_eq!(d.extras.props.note, None);
    assert_eq!(d.extras.props.transfer_out, None);
    assert_eq!(d.tree.note, "n");
    // Gates with a comment or paging are kept; plain and unnamed ones not.
    let kept: Vec<&str> = d.extras.gates.keys().map(String::as_str).collect();
    assert_eq!(kept, ["P", "Q"]);
    assert!(d.extras.gates["P"].is_paged && d.extras.gates["P"].comment.is_none());
    assert_eq!(d.extras.gates["Q"].comment.as_deref(), Some("c"));
}

#[test]
fn the_tree_decides_note_and_transfer_out() {
    use ftar_core::text::Extras;
    let d = parse_text(
        b"- times: 1\n\nGate: T\n- type: OR\n- inputs: A\n\nEvent: A\n- probability: 0.1\n",
    )
    .unwrap();
    let mut extras: Extras = d.extras.clone();
    extras.props.note = Some("stale".into());
    extras.props.transfer_out = Some("Stale".into());
    let text = write_text(&d.tree, &d.db, &extras).unwrap();
    assert!(!text.contains("stale") && !text.contains("Stale"), "{text}");
}

#[test]
fn other_top_gates_must_still_find_their_inputs() {
    // U is a second top gate and uses G; take G out of the tree.
    let src = "- times: 1\n\nGate: T\n- type: OR\n- inputs: G, B\n\nGate: U\n- type: AND\n- inputs: G, B\n\n\
Gate: G\n- type: AND\n- inputs: A, B\n\nEvent: A\n- probability: 0.1\n\nEvent: B\n- probability: 0.2\n";
    let d = parse_text(src.as_bytes()).unwrap();
    assert!(write_text(&d.tree, &d.db, &d.extras).is_ok());
    let mut tree = d.tree.clone();
    tree.root.children[0].children.remove(0);
    let e = write_text(&tree, &d.db, &d.extras).unwrap_err().to_string();
    assert!(e.contains("gate `U` uses `G`"), "{e}");
}

/// `n` untyped gates down to a VOTE(2) over an untyped gate `H` (over an
/// event) and two events.
fn vote_at_depth(n: usize) -> String {
    let mut src = String::new();
    for i in 0..n {
        src += &format!("Gate: G{i}\n- inputs: G{}\n\n", i + 1);
    }
    src += &format!("Gate: G{n}\n- type: VOTE(2)\n- inputs: H, E1, E2\n\n");
    src += "Gate: H\n- inputs: E0\n\nEvent: E0\n\nEvent: E1\n\nEvent: E2\n";
    src
}

fn parses_deep(src: String) -> bool {
    std::thread::Builder::new()
        .stack_size(16 << 20)
        .spawn(move || parse_text(src.as_bytes()).is_ok())
        .unwrap()
        .join()
        .unwrap()
}

#[test]
fn a_vote_expansion_counts_in_the_depth_limit() {
    // The VOTE adds an OR, an intermediate event and an AND above its inputs.
    assert!(parses_deep(vote_at_depth(993)));
    assert!(!parses_deep(vote_at_depth(994)));
    let d = parse_text(vote_at_depth(0).as_bytes()).unwrap();
    assert_eq!(d.tree.root.depth(), 6);
}

#[test]
fn a_vote_expansion_counts_in_the_node_limit() {
    use ftar_core::text::MAX_NODES;
    // VOTE(2) over n events: 2 n² - 2 n + 2 nodes.
    let vote = |n: usize| {
        let ids: Vec<String> = (0..n).map(|i| format!("E{i}")).collect();
        let mut src = format!("Gate: T\n- type: VOTE(2)\n- inputs: {}\n", ids.join(","));
        for id in &ids {
            src += &format!("\nEvent: {id}\n");
        }
        src
    };
    let d = parse_text(vote(4).as_bytes()).unwrap();
    assert_eq!(d.tree.nodes().len(), 26);
    assert!(2 * 224 * 224 - 2 * 224 + 2 <= MAX_NODES);
    assert!(parse_text(vote(224).as_bytes()).is_ok());
    assert!(parse_text(vote(225).as_bytes()).is_err());
}
