//! Fault tree to SVG. Layout comes from [`ftar_core::layout`], so the drawing
//! has the same structure as the GUI's canvas.

use std::fmt::Write as _;

use ftar_core::layout::{layout, Placed};
use ftar_core::{PrimaryEventDb, Tree};

use crate::symbols::{fit_text, node_text, symbol};
use crate::prefs::{hex, Preferences};

const NODE_W: f64 = 120.0;
/// Size of one symbol unit (a basic event is one unit across).
const UNIT: f64 = 100.0;
/// Room above the first row for ids.
const ID_SPACE: f64 = 16.0;
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

fn text(out: &mut String, x: f64, y: f64, size: f64, anchor: &str, s: &str, bold: bool) {
    let _ = writeln!(
        out,
        r#"<text x="{x:.1}" y="{y:.1}" font-size="{size}" text-anchor="{anchor}"{} >{}</text>"#,
        if bold { r#" font-weight="bold""# } else { "" },
        xml_escape(s)
    );
}

fn poly_points(pts: &[(f32, f32)], x: f64, y: f64) -> String {
    pts.iter()
        .map(|&(px, py)| format!("{:.1},{:.1}", x + px as f64 * UNIT, y + py as f64 * UNIT))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Draw `tree` as an SVG document. `db` supplies probabilities and descriptions.
///
/// Symbols and labels come from [`crate::symbols`], so the picture matches the GUI canvas
/// and OpenFTA: gate shapes with the id above, descriptions inside the boxes.
pub fn tree_svg(tree: &Tree, db: &PrimaryEventDb, opts: &SvgOptions) -> String {
    tree_svg_with(tree, db, opts, &Preferences::default())
}

/// Like [`tree_svg`], with the user's colours and monochrome mode.
pub fn tree_svg_with(
    tree: &Tree,
    db: &PrimaryEventDb,
    opts: &SvgOptions,
    prefs: &Preferences,
) -> String {
    let pen = hex(prefs.pen_colour());
    let placed = layout(&tree.root);
    let (max_x, max_y) = placed.iter().fold((0.0f64, 0.0f64), |a, p| {
        (a.0.max(p.x as f64), a.1.max(p.y as f64))
    });
    let title_h = if opts.title.is_empty() { 0.0 } else { TITLE_H };
    let width = max_x * GAP_X + NODE_W + 2.0 * MARGIN;
    let height = max_y * GAP_Y + UNIT + ID_SPACE + LABEL_SPACE + 2.0 * MARGIN + title_h;
    let origin_x = MARGIN;
    let origin_y = MARGIN + title_h + ID_SPACE;
    // centre column and top of a node's symbol
    let at = |p: &Placed| {
        (
            origin_x + p.x as f64 * GAP_X + NODE_W / 2.0,
            origin_y + p.y as f64 * GAP_Y,
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
        let _ = writeln!(
            o,
            r#"<rect width="100%" height="100%" fill="{}"/>"#,
            hex(prefs.background_colour())
        );
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
    let _ = writeln!(o, r#"<g fill="none" stroke="{pen}" stroke-width="1.6">"#);
    for p in &placed {
        if let Some(pi) = p.parent {
            let (ax, ay) = at(&placed[pi]);
            let (bx, by) = at(p);
            let ay = ay + symbol(placed[pi].node.kind).depth as f64 * UNIT;
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
        let (x, y) = at(p);
        let sym = symbol(n.kind);
        let lab = node_text(n, db);
        let stroke = format!(r#"stroke="{pen}" stroke-width="1.6""#);
        let dash = if sym.dashed {
            r#" stroke-dasharray="5 3""#
        } else {
            ""
        };
        let _ = writeln!(
            o,
            r#"<polygon points="{}" fill="{}" {stroke}{dash} stroke-linejoin="round"/>"#,
            poly_points(&sym.outline, x, y),
            hex(prefs.fill(n.kind))
        );
        for l in &sym.lines {
            let _ = writeln!(
                o,
                r#"<polyline points="{}" fill="none" {stroke}/>"#,
                poly_points(l, x, y)
            );
        }
        // id above the symbol, right of its centre (OpenFTA's draw_id)
        if !lab.id.is_empty() {
            text(&mut o, x + 5.0, y - 4.0, 12.0, "start", &lab.id, true);
        }
        let (tw, th) = sym.text_size;
        if tw > 0.0 && !lab.inside.is_empty() {
            let (lines, font) = fit_text(&lab.inside, tw * UNIT as f32, th * UNIT as f32, 10.0);
            let lh = font as f64 * 1.2;
            let cy = y + sym.text_centre.1 as f64 * UNIT;
            let top = cy - lh * lines.len() as f64 / 2.0 + font as f64 * 0.9;
            for (i, l) in lines.iter().enumerate() {
                text(
                    &mut o,
                    x,
                    top + i as f64 * lh,
                    font as f64,
                    "middle",
                    l,
                    false,
                );
            }
        }
        if opts.probabilities {
            if let Some(pt) = &lab.prob {
                text(
                    &mut o,
                    x,
                    y + sym.depth as f64 * UNIT + 14.0,
                    10.5,
                    "middle",
                    pt,
                    false,
                );
            }
        }
    }
    let _ = writeln!(o, "</svg>");
    o
}
