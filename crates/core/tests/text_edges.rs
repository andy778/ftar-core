#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! Limits and edge cases of the text format: exact boundaries, error
//! messages, and what the writer refuses (each pins down behaviour a
//! mutation test found unchecked).

use ftar_core::text::{parse_text, Extras, MAX_NODES};
use ftar_core::{Error, EventType, Node, PrimaryEventDb, Tree};

fn parse(s: &str) -> Result<(Tree, PrimaryEventDb), Error> {
    parse_text(s.as_bytes()).map(|d| (d.tree, d.db))
}

fn write_text(t: &Tree, db: &PrimaryEventDb) -> Result<String, Error> {
    ftar_core::text::write_text(t, db, &Extras::default())
}

fn parse_err(s: &str) -> String {
    parse(s).unwrap_err().to_string()
}

const SMALL: &str = "Gate: T\n- type: OR\n- inputs: A, B\n\nEvent: A\n- probability: 0.1\n\nEvent: B\n- probability: 0.2\n";

#[test]
fn errors_name_the_line() {
    let e = parse_err("Gate: T\n- inputs: A\n\n- label: x\n");
    assert!(e.contains("line 4:"), "{e}");
}

#[test]
fn ids_are_checked_character_by_character() {
    let long = "x".repeat(ftar_core::database::MAX_TEXT);
    assert!(parse(&format!("Gate: {long}\n")).is_ok());
    assert!(parse_err(&format!("Gate: {long}y\n")).contains("bad id"));
    for bad in ["a,b", "a;b", "a\u{7}b", "NULL"] {
        let e = parse_err(&format!("Gate: {bad}\n"));
        assert!(e.contains("bad id"), "{bad:?}: {e}");
    }
}

#[test]
fn unknown_types_list_the_known_ones() {
    let e = parse_err("Gate: T\n- type: NAND\n");
    assert!(
        e.contains("AND, OR, VOTE(k), NULL, XOR, INHIBIT or PRIORITY_AND"),
        "{e}"
    );
    let e = parse_err("Gate: T\n- inputs: A\n\nEvent: A\n- type: dog\n");
    assert!(e.contains("basic, undeveloped, house"), "{e}");
}

#[test]
fn false_flags_are_accepted() {
    let (t, _) =
        parse("Gate: T\n- unnamed: false\n- inputs: A\n\nEvent: A\n- dormant: false\n").unwrap();
    assert_eq!(t.root.id, "T");
    assert_eq!(t.root.children[0].modifier, None);
}

/// A top OR gate over `n` events: `n + 2` nodes.
fn wide(n: usize) -> String {
    let ids: Vec<String> = (0..n).map(|i| format!("E{i}")).collect();
    let mut src = format!("Gate: T\n- type: OR\n- inputs: {}\n", ids.join(","));
    for id in &ids {
        src += &format!("\nEvent: {id}\n");
    }
    src
}

#[test]
fn the_node_limit_is_exact() {
    assert!(parse(&wide(MAX_NODES - 2)).is_ok());
    assert!(parse_err(&wide(MAX_NODES - 1)).contains(&MAX_NODES.to_string()));
}

/// A chain of `n` gates (typed or not) ending in one event.
fn chain(n: usize, typed: bool) -> String {
    let t = if typed { "- type: AND\n" } else { "" };
    let mut src = String::new();
    for i in 0..n {
        src += &format!("Gate: G{i}\n{t}- inputs: G{}\n\n", i + 1);
    }
    src + &format!("Event: G{n}\n")
}

fn parses_deep(src: String) -> bool {
    std::thread::Builder::new()
        .stack_size(16 << 20)
        .spawn(move || parse(&src).is_ok())
        .unwrap()
        .join()
        .unwrap()
}

#[test]
fn the_depth_limit_is_exact() {
    // Untyped gates add one level each, typed ones two (the gate symbol).
    assert!(parses_deep(chain(998, false)));
    assert!(!parses_deep(chain(999, false)));
    assert!(parses_deep(chain(499, true)));
    assert!(!parses_deep(chain(500, true)));
}

#[test]
fn numbers_are_written_short() {
    let src = "Gate: T\n- type: OR\n- inputs: A, B, C\n\nEvent: A\n- probability: 0\n\n\
Event: B\n- probability: 0.05\n\nEvent: C\n- probability: 1e-7\n";
    let (t, db) = parse(src).unwrap();
    let text = write_text(&t, &db).unwrap();
    for want in [
        "- probability: 0\n",
        "- probability: 0.05\n",
        "- probability: 1e-7\n",
    ] {
        assert!(text.contains(want), "{want:?} in\n{text}");
    }
}

#[test]
fn group_and_dormant_survive() {
    let src = "Gate: T\n- type: OR\n- inputs: A\n\nEvent: A\n- dormant: true\n- group: pumps\n- probability: 0.1\n";
    let (t, db) = parse(src).unwrap();
    assert_eq!(t.root.children[0].children[0].modifier, Some('D'));
    let text = write_text(&t, &db).unwrap();
    assert!(text.contains("- group: pumps\n"), "{text}");
    assert!(text.contains("- dormant: True\n"), "{text}");
    assert_eq!(parse(&text).unwrap(), (t, db));
}

#[test]
fn writer_refuses_inputs_that_cannot_be_inputs() {
    for kind in [EventType::And, EventType::TransferOut, EventType::Unknown] {
        let (mut t, db) = parse(SMALL).unwrap();
        t.root.children[0].children.push(Node {
            kind,
            modifier: None,
            id: "X".into(),
            text: String::new(),
            children: Vec::new(),
        });
        let e = write_text(&t, &db).unwrap_err().to_string();
        assert!(e.contains("cannot be an input"), "{kind:?}: {e}");
    }
}

#[test]
fn writer_refuses_one_event_with_two_types() {
    let (mut t, db) = parse(SMALL).unwrap();
    let mut b = t.root.children[0].children[1].clone();
    b.id = "A".into();
    b.kind = EventType::Undeveloped;
    t.root.children[0].children[1] = b.clone();
    assert!(write_text(&t, &db).is_err());
    b.kind = EventType::Basic;
    b.modifier = Some('D');
    t.root.children[0].children[1] = b;
    assert!(write_text(&t, &db).is_err());
}

#[test]
fn unnamed_names_skip_ids_in_use() {
    // `unnamed_1` is taken by an event, so the unnamed top gets `unnamed_2`.
    let src = "Gate: x\n- unnamed: true\n- type: OR\n- inputs: unnamed_1\n\nEvent: unnamed_1\n";
    let (t, db) = parse(src).unwrap();
    let text = write_text(&t, &db).unwrap();
    assert!(text.contains("Gate: unnamed_2\n"), "{text}");
    assert_eq!(parse(&text).unwrap(), (t, db));
}

#[test]
fn carriage_returns_at_line_ends_do_not_change_on_rewrite() {
    // Found by fuzzing: only one of several CRs was dropped on each read.
    let src = "- note: a\rb\r\r\r\n- note: c\r\r\r\r\n\nGate: T\n- label: x\r\r\r\n";
    let (t, db) = parse(src).unwrap();
    assert_eq!(t.note, "a\rb\nc");
    assert_eq!(t.root.text, "x");
    let text = write_text(&t, &db).unwrap();
    assert_eq!(parse(&text).unwrap(), (t, db));
}
