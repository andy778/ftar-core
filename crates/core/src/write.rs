//! Writers for the OpenFTA file formats (the inverse of [`crate::parse`]).
//!
//! The output follows `save_item` / `primary_event_write` of the original
//! sources, so the original OpenFTA can read it back.

use crate::model::{EventType, Node, PrimaryEventDb, Tree};

/// Line terminator used when writing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineEnding {
    /// `\n`, as in the example files shipped with OpenFTA (default).
    #[default]
    Lf,
    /// `\r\n`, as the original produces on Windows.
    CrLf,
}

impl LineEnding {
    fn as_str(self) -> &'static str {
        match self {
            LineEnding::Lf => "\n",
            LineEnding::CrLf => "\r\n",
        }
    }
}

/// Serialize a tree to `.fta` text with LF line endings.
pub fn write_fta(tree: &Tree) -> String {
    write_fta_with(tree, LineEnding::Lf)
}

/// Serialize a tree to `.fta` text with the given line endings.
pub fn write_fta_with(tree: &Tree, eol: LineEnding) -> String {
    let nl = eol.as_str();
    let mut out = String::new();
    out.push_str(
        tree.db_name
            .as_deref()
            .filter(|s| !s.is_empty())
            .unwrap_or("NULL"),
    );
    out.push_str(nl);
    out.push_str("S NULL 0");
    out.push_str(nl);
    // The length counts a line break as one character (see the reader).
    out.push_str(&tree.note.len().to_string());
    if !tree.note.is_empty() {
        out.push(' ');
        out.push_str(&convert_eol(&tree.note, eol));
    }
    out.push_str(nl);
    write_node(&tree.root, nl, &mut out);
    out
}

fn convert_eol(s: &str, eol: LineEnding) -> String {
    match eol {
        LineEnding::Lf => s.to_string(),
        LineEnding::CrLf => s.replace("\r\n", "\n").replace('\n', "\r\n"),
    }
}

fn write_node(n: &Node, nl: &str, out: &mut String) {
    out.push(n.kind.code());
    if let Some(m) = n.modifier {
        out.push(m);
    }
    out.push(' ');
    out.push_str(if n.id.is_empty() { "NULL" } else { &n.id });
    out.push(' ');
    out.push_str(&n.children.len().to_string());
    out.push(' ');
    out.push_str(nl);
    if n.kind == EventType::Intermediate {
        out.push_str(&n.text.len().to_string());
        if !n.text.is_empty() {
            out.push(' ');
            out.push_str(&n.text);
        }
        out.push_str(nl);
    }
    for c in &n.children {
        write_node(c, nl, out);
    }
}

/// Format like C's `%e` (six decimals, at least two exponent digits) when
/// that is lossless, else with the shortest exact representation.
fn format_prob(p: f64) -> String {
    fn fix(s: String) -> String {
        match s.split_once('e') {
            Some((m, e)) => {
                let (sign, digits) = match e.strip_prefix('-') {
                    Some(d) => ('-', d),
                    None => ('+', e),
                };
                format!("{m}e{sign}{digits:0>2}")
            }
            None => s,
        }
    }
    if !p.is_finite() {
        return "0.000000e+00".into();
    }
    let c = format!("{p:.6e}");
    if c.parse::<f64>().ok() == Some(p) {
        fix(c)
    } else {
        fix(format!("{p:e}"))
    }
}

/// Serialize a primary event database to `.ped` text (LF line endings),
/// events sorted by id. Absent fields are written as NUL bytes.
pub fn write_ped(db: &PrimaryEventDb) -> String {
    let mut ids: Vec<&String> = db.events.keys().collect();
    ids.sort();
    let mut out = String::new();
    for id in ids {
        let e = &db.events[id];
        out.push_str(&e.id);
        out.push(';');
        out.push_str(&e.group);
        out.push(';');
        out.push(e.kind.code());
        out.push(e.modifier.unwrap_or('\0'));
        out.push(';');
        out.push_str(&e.description);
        out.push(';');
        out.push_str(&format_prob(e.probability));
        out.push(';');
        out.push(e.lambda.unwrap_or('\0'));
        out.push_str(";\n");
    }
    out
}
