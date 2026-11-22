//! The standard fault tree symbols, as OpenFTA draws them, and the labels that go with them.
//!
//! Geometry follows `src/lib/Appn/symbol_defs.h` of OpenFTA (GPL): every symbol is
//! given in unit coordinates, `x` centred on 0 and `y` growing downwards from the top
//! of the symbol. A drawing backend (SVG here, egui in the GUI) multiplies by its own
//! unit size, so image export, print and the canvas show the same shapes.

use ftar_core::{EventType, Node, PrimaryEventDb};

use crate::svg::wrap;

pub type Pt = (f32, f32);

/// One symbol in unit coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct Symbol {
    /// Closed outline (may be concave).
    pub outline: Vec<Pt>,
    /// The inside as convex pieces, for backends that can only fill convex polygons.
    pub fills: Vec<Vec<Pt>>,
    /// Extra open lines drawn on top (the stem of a transfer symbol, the V of PAND/XOR).
    pub lines: Vec<Vec<Pt>>,
    /// Dashed outline (dormant events and unanalysed conditions).
    pub dashed: bool,
    pub width: f32,
    pub depth: f32,
    /// Where descriptions go: centre and size; zero size for symbols without text.
    pub text_centre: Pt,
    pub text_size: (f32, f32),
}

const ARC_STEPS: usize = 24;

fn ellipse(cx: f32, cy: f32, rx: f32, ry: f32) -> Vec<Pt> {
    (0..ARC_STEPS * 2)
        .map(|i| {
            let t = i as f32 / (ARC_STEPS * 2) as f32 * std::f32::consts::TAU;
            (cx + rx * t.cos(), cy + ry * t.sin())
        })
        .collect()
}

/// Upper half of an ellipse standing on `base_y`, from the right end to the left end.
fn dome(cx: f32, base_y: f32, rx: f32, h: f32) -> Vec<Pt> {
    (0..=ARC_STEPS)
        .map(|i| {
            let t = i as f32 / ARC_STEPS as f32 * std::f32::consts::PI;
            (cx + rx * t.cos(), base_y - h * t.sin())
        })
        .collect()
}

fn convex(points: &[Pt], depth: f32, width: f32, text: (Pt, (f32, f32))) -> Symbol {
    Symbol {
        outline: points.to_vec(),
        fills: vec![points.to_vec()],
        lines: Vec::new(),
        dashed: false,
        width,
        depth,
        text_centre: text.0,
        text_size: text.1,
    }
}

const NO_TEXT: (Pt, (f32, f32)) = ((0.0, 0.0), (0.0, 0.0));
const V: [Pt; 3] = [(-0.2, 0.4), (0.0, 0.0), (0.2, 0.4)];

fn and_shape() -> Symbol {
    let mut o = dome(0.0, 0.28, 0.2, 0.28);
    o.push((-0.2, 0.4));
    o.push((0.2, 0.4));
    convex(&o, 0.4, 0.4, NO_TEXT)
}

fn or_shape() -> Symbol {
    let mut o = dome(0.0, 0.4, 0.2, 0.4);
    // concave bottom edge, left to right
    o.extend(dome(0.0, 0.4, 0.2, 0.05).into_iter().rev());
    let n = 16;
    let s = |x: f32| (1.0 - (x / 0.2) * (x / 0.2)).max(0.0).sqrt();
    let fills = (0..n)
        .map(|i| {
            let a = -0.2 + 0.4 * i as f32 / n as f32;
            let b = -0.2 + 0.4 * (i + 1) as f32 / n as f32;
            vec![
                (a, 0.4 - 0.4 * s(a)),
                (b, 0.4 - 0.4 * s(b)),
                (b, 0.4 - 0.05 * s(b)),
                (a, 0.4 - 0.05 * s(a)),
            ]
        })
        .collect();
    Symbol {
        outline: o,
        fills,
        ..convex(&[], 0.4, 0.4, NO_TEXT)
    }
}

/// The unit-coordinate symbol for an event or gate type.
pub fn symbol(kind: EventType) -> Symbol {
    use EventType::*;
    match kind {
        Basic | Dormant => {
            let mut s = convex(
                &ellipse(0.0, 0.5, 0.5, 0.5),
                1.0,
                1.0,
                ((0.0, 0.5), (0.7, 0.7)),
            );
            s.dashed = kind == Dormant;
            s
        }
        Undeveloped => convex(
            &[(0.0, 0.0), (-0.65, 0.4), (0.0, 0.8), (0.65, 0.4)],
            0.8,
            1.3,
            ((0.0, 0.4), (0.7, 0.5)),
        ),
        Intermediate | Unknown => convex(
            &[(-0.52, 0.0), (0.52, 0.0), (0.52, 0.7), (-0.52, 0.7)],
            0.7,
            1.04,
            ((0.0, 0.35), (0.97, 0.64)),
        ),
        And => and_shape(),
        PriorityAnd => {
            let mut s = and_shape();
            s.lines.push(V.to_vec());
            s
        }
        Or => or_shape(),
        Xor => {
            let mut s = or_shape();
            s.lines.push(V.to_vec());
            s
        }
        Inhibit => convex(
            &[
                (0.0, 0.0),
                (0.2, 0.12),
                (0.2, 0.4),
                (0.0, 0.52),
                (-0.2, 0.4),
                (-0.2, 0.12),
            ],
            0.52,
            0.4,
            NO_TEXT,
        ),
        External => convex(
            &[
                (0.0, 0.0),
                (0.42, 0.12),
                (0.42, 1.0),
                (-0.42, 1.0),
                (-0.42, 0.12),
            ],
            1.0,
            0.84,
            ((0.0, 0.56), (0.8, 0.8)),
        ),
        ConditionAnalysed | ConditionNotAnalysed => {
            let mut s = convex(
                &ellipse(0.0, 0.32, 0.52, 0.32),
                0.64,
                1.04,
                ((0.0, 0.32), (0.9, 0.5)),
            );
            s.dashed = kind == ConditionNotAnalysed;
            s
        }
        TransferIn => {
            let mut s = convex(
                &[(0.0, 0.18), (0.21, 0.51), (-0.21, 0.51)],
                0.51,
                0.42,
                NO_TEXT,
            );
            s.lines.push(vec![(0.0, 0.0), (0.0, 0.18)]);
            s
        }
        TransferOut => {
            let mut s = convex(
                &[(-0.06, 0.09), (0.0, 0.0), (0.21, 0.33), (-0.21, 0.33)],
                0.33,
                0.42,
                NO_TEXT,
            );
            s.lines.push(vec![(-0.06, 0.09), (-0.21, 0.09)]);
            s
        }
    }
}

/// Text next to a symbol.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NodeText {
    /// Event or gate id; drawn above the symbol, right of its centre.
    pub id: String,
    /// Description, drawn inside the symbol.
    pub inside: String,
    /// Probability (or rate) of a primary event.
    pub prob: Option<String>,
}

/// `p = 3.00E-2` or `rate 2.00E-2` for an event in the database.
pub fn prob_text(db: &PrimaryEventDb, id: &str) -> Option<String> {
    let e = db.events.get(id)?;
    let v = format!("{:.2E}", e.probability);
    Some(if e.lambda == Some('L') {
        format!("rate {v}")
    } else {
        format!("p = {v}")
    })
}

/// Labels for a node, the way OpenFTA prints them: the id for every item, the tree's
/// description for intermediate events, the database's for primary events and conditions.
pub fn node_text(n: &Node, db: &PrimaryEventDb) -> NodeText {
    let has_db_entry = n.kind.is_primary() || n.kind == EventType::ConditionAnalysed;
    let inside = match n.kind {
        EventType::Intermediate => n.text.clone(),
        _ if has_db_entry => db
            .events
            .get(&n.id)
            .map(|e| e.description.clone())
            .unwrap_or_default(),
        _ => String::new(),
    };
    NodeText {
        id: n.id.clone(),
        inside,
        prob: if has_db_entry {
            prob_text(db, &n.id)
        } else {
            None
        },
    }
}

/// Header block of OpenFTA's canvas: `Tree:`, `Database:` and the description.
pub fn header_lines(tree_name: &str, db_name: Option<&str>, note: &str) -> Vec<String> {
    let mut v = vec![format!("Tree: {tree_name}")];
    if let Some(d) = db_name.filter(|d| !d.is_empty()) {
        v.push(format!("Database: {d}"));
    }
    let note = note.split_whitespace().collect::<Vec<_>>().join(" ");
    if !note.is_empty() {
        v.push(note);
    }
    v
}

/// Lay a description out inside a box `w` x `h` (same unit as `max_font`). Picks the largest
/// font (down to 60 percent of `max_font`) that shows the whole text; at the smallest, text
/// that still does not fit is cut with an ellipsis. Returns the lines and the font size.
pub fn fit_text(text: &str, w: f32, h: f32, max_font: f32) -> (Vec<String>, f32) {
    if !(w > 0.0 && h > 0.0 && max_font > 0.0) || text.trim().is_empty() {
        return (Vec::new(), max_font.max(0.0));
    }
    let min_font = max_font * 0.6;
    let mut font = max_font;
    loop {
        let chars = ((w / (font * 0.55)).floor() as usize).max(1);
        let rows = ((h / (font * 1.2)).floor() as usize).max(1);
        let all = wrap(text, chars, usize::MAX);
        if all.len() <= rows {
            return (all, font);
        }
        if font <= min_font + 1e-3 {
            return (wrap(text, chars, rows), font);
        }
        font = (font - 0.25).max(min_font);
    }
}
