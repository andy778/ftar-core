//! Fault tree to SVG: the [`crate::scene`] drawing written out as SVG, so
//! images and prints look like the GUI's canvas.

use std::fmt::Write as _;

use ftar_core::{PrimaryEventDb, Tree};

use crate::prefs::{hex, Preferences};
use crate::scene::{tree_scene, Anchor, Prim, SceneOptions};

pub use crate::scene::wrap;

const FONT: &str = "DejaVu Sans, Arial, Helvetica, sans-serif";

#[derive(Debug, Clone)]
pub struct SvgOptions {
    /// Tree name; when not empty a header with it, the database and the
    /// tree's note is drawn above the tree.
    pub title: String,
    /// Show each primary event's probability (or rate) by it.
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

fn points(pts: &[[f32; 2]]) -> String {
    pts.iter()
        .map(|p| format!("{:.1},{:.1}", p[0], p[1]))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Draw `tree` as an SVG document. `db` supplies descriptions and probabilities.
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
    let scene = tree_scene(
        tree,
        db,
        &SceneOptions {
            title: opts.title.clone(),
            probabilities: opts.probabilities,
            prefs: prefs.clone(),
        },
    );
    let (pen, paper) = (hex(prefs.pen_colour()), hex(prefs.background_colour()));
    let mut o = String::new();
    let _ = writeln!(
        o,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w:.0}" height="{h:.0}" viewBox="0 0 {w:.0} {h:.0}" font-family="{FONT}">"#,
        w = scene.width.ceil(),
        h = scene.height.ceil()
    );
    if opts.background {
        let _ = writeln!(o, r#"<rect width="100%" height="100%" fill="{}"/>"#, paper);
    }
    let _ = writeln!(
        o,
        r#"<g stroke="{}" stroke-width="1.2" stroke-linejoin="round">"#,
        pen
    );
    for prim in &scene.prims {
        match prim {
            Prim::Path {
                pts,
                closed,
                fill,
                dashed,
            } => {
                let tag = if *closed { "polygon" } else { "polyline" };
                let fill = fill.map_or("none".to_string(), |f| f.hex());
                let dash = if *dashed {
                    r#" stroke-dasharray="5 3""#
                } else {
                    ""
                };
                let _ = writeln!(
                    o,
                    r#"<{tag} points="{}" fill="{fill}"{dash}/>"#,
                    points(pts)
                );
            }
            Prim::Text {
                pos,
                size,
                anchor,
                bold,
                text,
            } => {
                let anchor = match anchor {
                    Anchor::Start => "start",
                    Anchor::Middle => "middle",
                };
                // pos is on the line's vertical centre; SVG wants the baseline
                let _ = writeln!(
                    o,
                    r#"<text x="{:.1}" y="{:.1}" font-size="{size}" text-anchor="{anchor}" stroke="none" fill="{}"{}>{}</text>"#,
                    pos[0],
                    pos[1] + size * 0.36,
                    pen,
                    if *bold { r#" font-weight="bold""# } else { "" },
                    xml_escape(text)
                );
            }
        }
    }
    let _ = writeln!(o, "</g>");
    let _ = writeln!(o, "</svg>");
    o
}
