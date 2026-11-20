//! Fault tree to SVG. Layout comes from [`ftar_core::layout`], so the drawing
//! has the same structure as the GUI's canvas.

use std::fmt::Write as _;

use ftar_core::layout::{layout, Placed};
use ftar_core::{EventType, PrimaryEventDb, Tree};

const NODE_W: f64 = 120.0;
const NODE_H: f64 = 56.0;
const GAP_X: f64 = 140.0;
const GAP_Y: f64 = 120.0;
const MARGIN: f64 = 30.0;
/// Room under a node for its id, probability and caption lines.
const LABEL_SPACE: f64 = 70.0;
const TITLE_H: f64 = 60.0;
const FONT: &str = "DejaVu Sans, Arial, Helvetica, sans-serif";

#[derive(Debug, Clone)]
pub struct SvgOptions {
    /// Drawn above the tree when not empty.
    pub title: String,
    /// Show each primary event's probability (or rate) under it.
    pub probabilities: bool,
    /// Draw a white background (turn off for transparent PNGs).
    pub background: bool,
}

impl Default for SvgOptions {
    fn default() -> Self {
        Self {
            title: String::new(),
            probabilities: true,
            background: true,
        }
    }
}

/// Escape text for XML content and attributes.
pub fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            // characters XML 1.0 cannot carry
            c if (c as u32) < 0x20 && !matches!(c, '\t' | '\n' | '\r') => {}
            c => out.push(c),
        }
    }
    out
}

/// Split `text` into lines of at most `width` characters, at most `max_lines`
/// of them (the last one ends with an ellipsis when text was cut).
pub fn wrap(text: &str, width: usize, max_lines: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut cur = String::new();
    for word in text.split_whitespace() {
        let wlen = word.chars().count();
        let clen = cur.chars().count();
        if clen > 0 && clen + 1 + wlen > width {
            lines.push(std::mem::take(&mut cur));
        }
        if !cur.is_empty() {
            cur.push(' ');
        }
        // a single over-long word is cut
        if wlen > width {
            cur.push_str(
                &word
                    .chars()
                    .take(width.saturating_sub(1))
                    .collect::<String>(),
            );
            cur.push('…');
        } else {
            cur.push_str(word);
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    if lines.len() > max_lines {
        lines.truncate(max_lines);
        if let Some(last) = lines.last_mut() {
            let keep = width.saturating_sub(1);
            *last = last.chars().take(keep).collect::<String>() + "…";
        }
    }
    lines
}

fn fill(kind: EventType) -> &'static str {
    use EventType::*;
    match kind {
        Basic => "#a8c4ea",
        Dormant => "#bcbcec",
        Undeveloped => "#ecd9a0",
        External => "#a9d9bc",
        And => "#e8a8a8",
        Or => "#b0d6b0",
        Xor | PriorityAnd | Inhibit => "#d8b4d8",
        Intermediate => "#c8c8c8",
        ConditionAnalysed | ConditionNotAnalysed => "#f0e0c0",
        TransferIn | TransferOut => "#d0d0d0",
        Unknown => "#dddddd",
    }
}

fn gate_label(kind: EventType) -> &'static str {
    use EventType::*;
    match kind {
        And => "AND",
        Or => "OR",
        Xor => "XOR",
        PriorityAnd => "PAND",
        Inhibit => "INH",
        _ => "",
    }
}

fn prob_text(db: &PrimaryEventDb, id: &str) -> Option<String> {
    let e = db.events.get(id)?;
    let v = format!("{:.2E}", e.probability);
    Some(if e.lambda == Some('L') {
        format!("rate {v}")
    } else {
        format!("p = {v}")
    })
}

fn text(out: &mut String, x: f64, y: f64, size: f64, anchor: &str, s: &str, bold: bool) {
    let _ = writeln!(
        out,
        r#"<text x="{x:.1}" y="{y:.1}" font-size="{size}" text-anchor="{anchor}"{} >{}</text>"#,
        if bold { r#" font-weight="bold""# } else { "" },
        xml_escape(s)
    );
}

/// Draw `tree` as an SVG document. `db` supplies probabilities and descriptions.
pub fn tree_svg(tree: &Tree, db: &PrimaryEventDb, opts: &SvgOptions) -> String {
    let placed = layout(&tree.root);
    let (max_x, max_y) = placed.iter().fold((0.0f64, 0.0f64), |a, p| {
        (a.0.max(p.x as f64), a.1.max(p.y as f64))
    });
    let title_h = if opts.title.is_empty() { 0.0 } else { TITLE_H };
    let width = max_x * GAP_X + NODE_W + 2.0 * MARGIN;
    let height = max_y * GAP_Y + NODE_H + LABEL_SPACE + 2.0 * MARGIN + title_h;
    let origin_x = MARGIN;
    let origin_y = MARGIN + title_h;
    // centre of a node
    let c = |p: &Placed| {
        (
            origin_x + p.x as f64 * GAP_X + NODE_W / 2.0,
            origin_y + p.y as f64 * GAP_Y + NODE_H / 2.0,
        )
    };

    let mut o = String::new();
    let _ = writeln!(
        o,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w:.0}" height="{h:.0}" viewBox="0 0 {w:.0} {h:.0}" font-family="{FONT}">"#,
        w = width.ceil(),
        h = height.ceil()
    );
    if opts.background {
        let _ = writeln!(o, r##"<rect width="100%" height="100%" fill="#ffffff"/>"##);
    }
    if !opts.title.is_empty() {
        text(
            &mut o,
            MARGIN,
            MARGIN + 22.0,
            22.0,
            "start",
            &opts.title,
            true,
        );
        // The primary event database the tree takes its events from, as in OpenFTA's printout.
        if let Some(db) = tree.db_name.as_deref().filter(|n| !n.is_empty()) {
            text(
                &mut o,
                MARGIN,
                MARGIN + 44.0,
                14.0,
                "start",
                &format!("Database: {db}"),
                false,
            );
        }
    }

    // connectors first, so shapes sit on top
    let _ = writeln!(
        o,
        r##"<g fill="none" stroke="#555555" stroke-width="1.6">"##
    );
    for p in &placed {
        if let Some(pi) = p.parent {
            let (ax, ay) = c(&placed[pi]);
            let (bx, by) = c(p);
            let mut start = ay + NODE_H / 2.0 - 6.0;
            let parent = placed[pi].node;
            if parent.kind == EventType::Intermediate {
                // start below the caption instead of running through it
                let lines = wrap(&parent.text, 22, 3).len();
                if lines > 0 {
                    start = ay + NODE_H * 0.35 + 14.0 + (lines - 1) as f64 * 13.0 + 5.0;
                }
            }
            let (ay, by) = (start, by - NODE_H / 2.0 + 6.0);
            let mid = (ay + by) / 2.0;
            let _ = writeln!(
                o,
                r#"<polyline points="{ax:.1},{ay:.1} {ax:.1},{mid:.1} {bx:.1},{mid:.1} {bx:.1},{by:.1}"/>"#
            );
        }
    }
    let _ = writeln!(o, "</g>");

    for p in &placed {
        let n = p.node;
        let (x, y) = c(p);
        let f = fill(n.kind);
        let stroke = r##"stroke="#444444" stroke-width="1.6""##;
        use EventType::*;
        // first free text baseline under the shape (NaN: no label)
        let label_y: f64 = match n.kind {
            Basic | Dormant => {
                let dash = if n.kind == Dormant {
                    r#" stroke-dasharray="5 3""#
                } else {
                    ""
                };
                let _ = writeln!(
                    o,
                    r#"<circle cx="{x:.1}" cy="{y:.1}" r="{r}" fill="{f}" {stroke}{dash}/>"#,
                    r = NODE_H / 2.0 - 6.0
                );
                y + NODE_H / 2.0 + 8.0
            }
            Undeveloped => {
                let (w, h) = (NODE_H * 0.6, NODE_H * 0.45);
                let _ = writeln!(
                    o,
                    r#"<polygon points="{x:.1},{:.1} {:.1},{y:.1} {x:.1},{:.1} {:.1},{y:.1}" fill="{f}" {stroke}/>"#,
                    y - h,
                    x + w,
                    y + h,
                    x - w
                );
                y + h + 14.0
            }
            External => {
                let (w, h) = (NODE_H * 0.5, NODE_H * 0.45);
                let _ = writeln!(
                    o,
                    r#"<polygon points="{x:.1},{:.1} {:.1},{y:.1} {:.1},{:.1} {:.1},{:.1} {:.1},{y:.1}" fill="{f}" {stroke}/>"#,
                    y - h,
                    x + w,
                    x + w,
                    y + h,
                    x - w,
                    y + h,
                    x - w
                );
                y + h + 14.0
            }
            ConditionAnalysed | ConditionNotAnalysed => {
                let dash = if n.kind == ConditionNotAnalysed {
                    r#" stroke-dasharray="4 3""#
                } else {
                    ""
                };
                let _ = writeln!(
                    o,
                    r#"<ellipse cx="{x:.1}" cy="{y:.1}" rx="{:.1}" ry="{:.1}" fill="{f}" {stroke}{dash}/>"#,
                    NODE_W * 0.4,
                    NODE_H * 0.3
                );
                y + NODE_H * 0.3 + 16.0
            }
            TransferIn | TransferOut => {
                let (w, h) = (NODE_H * 0.45, NODE_H * 0.4);
                let _ = writeln!(
                    o,
                    r#"<polygon points="{x:.1},{:.1} {:.1},{:.1} {:.1},{:.1}" fill="{f}" {stroke}/>"#,
                    y - h,
                    x + w,
                    y + h,
                    x - w,
                    y + h
                );
                y + h + 16.0
            }
            k if k.is_gate() => {
                let (w, h) = (NODE_H * 0.9, NODE_H * 0.55);
                let _ = writeln!(
                    o,
                    r#"<rect x="{:.1}" y="{:.1}" width="{w:.1}" height="{h:.1}" rx="14" fill="{f}" {stroke}/>"#,
                    x - w / 2.0,
                    y - h / 2.0
                );
                text(&mut o, x, y + 5.0, 14.0, "middle", gate_label(k), false);
                f64::NAN // gates carry no id label
            }
            _ => {
                let (w, h) = (NODE_W * 0.92, NODE_H * 0.7);
                let _ = writeln!(
                    o,
                    r#"<rect x="{:.1}" y="{:.1}" width="{w:.1}" height="{h:.1}" rx="4" fill="{f}" {stroke}/>"#,
                    x - w / 2.0,
                    y - h / 2.0
                );
                y + h / 2.0 + 14.0
            }
        };
        if n.kind == Intermediate {
            // id inside the box, description under it
            if !n.id.is_empty() {
                text(&mut o, x, y + 5.0, 13.0, "middle", &n.id, true);
            }
            for (i, l) in wrap(&n.text, 22, 3).iter().enumerate() {
                text(
                    &mut o,
                    x,
                    label_y + i as f64 * 13.0,
                    11.0,
                    "middle",
                    l,
                    false,
                );
            }
        } else if !label_y.is_nan() {
            if !n.id.is_empty() {
                text(&mut o, x, label_y, 12.0, "middle", &n.id, true);
            }
            if opts.probabilities && (n.kind.is_primary() || n.kind == ConditionAnalysed) {
                if let Some(pt) = prob_text(db, &n.id) {
                    text(&mut o, x, label_y + 13.0, 10.5, "middle", &pt, false);
                }
            }
        }
    }
    let _ = writeln!(o, "</svg>");
    o
}
