//! A fault tree drawn the way OpenFTA draws it, as a list of backend-neutral
//! shapes and text. The SVG export ([`crate::svg`]) and the GUI canvas both
//! draw from it, so they look the same.
//!
//! What follows OpenFTA (`src/lib/Appn/symbol_defs.h`, `display_item.c`,
//! `symbol_text.c`, `treeUtil.h`):
//! - the twelve symbols, with OpenFTA's proportions: AND and priority AND
//!   gates are domes, OR and XOR gates are pointed arches with a curved base,
//!   the inhibit gate a hexagon, basic events circles, undeveloped events
//!   diamonds, external events houses, conditions ellipses, transfers
//!   triangles; dormant events get a second, larger circle;
//! - the layout grid: 1.4 units between columns, 1 unit between rows, every
//!   symbol hanging from its top centre; conditions and transfer-out symbols
//!   sit to the right of their parent instead of under it;
//! - connectors: a stub of 0.25 units up from each child to a horizontal bar,
//!   and a line down from the parent to the bar;
//! - labels: each item's id above its symbol, right of the connector; a
//!   primary event's probability under the id; the description (intermediate
//!   events and conditions from the tree, primary events from the database)
//!   wrapped inside the symbol; gates show only their id;
//! - a header with the tree name, its database and the tree's note.
//!
//! Coordinates are pixels at 100 % zoom, y down.

use ftar_core::{EventType, Node, PrimaryEventDb, Tree};

/// Pixels per OpenFTA layout unit.
pub const UNIT: f32 = 100.0;
/// Font size of labels and descriptions.
pub const FONT: f32 = 10.0;
const LINE_H: f32 = 12.0;
const HEADER_FONT: f32 = 12.0;
const HEADER_LINE_H: f32 = 15.0;
const COL: f32 = 1.4 * UNIT;
const ROW: f32 = 1.0 * UNIT;
const CONNECT: f32 = 0.25 * UNIT;
const MARGIN: f32 = 24.0;
/// Average glyph width as a fraction of the font size, for wrapping and bounds.
const CHAR_W: f32 = 0.55;
/// Segments used to draw a full ellipse.
const ELLIPSE_STEPS: usize = 48;

/// RGB colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    pub fn hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.0, self.1, self.2)
    }
}

/// OpenFTA's default colours (`Preferences.java`).
pub const PEN: Rgb = Rgb(0, 0, 0);
pub const PAPER: Rgb = Rgb(255, 255, 255);
/// The pen colour OpenFTA highlights the selected item with.
pub const HIGHLIGHT: Rgb = Rgb(0, 160, 0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anchor {
    Start,
    Middle,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Prim {
    /// A polygon (`closed`) or polyline, filled when `fill` is set.
    Path {
        pts: Vec<[f32; 2]>,
        closed: bool,
        fill: Option<Rgb>,
        dashed: bool,
    },
    /// One line of text; `pos` is the anchor point on the line's vertical centre.
    Text {
        pos: [f32; 2],
        size: f32,
        anchor: Anchor,
        bold: bool,
        text: String,
    },
}

/// One tree item in the scene.
#[derive(Debug, Clone)]
pub struct Item {
    /// Child indices from the top event, as in [`ftar_core::edit`].
    pub path: Vec<usize>,
    /// The symbol's bounding box: min x, min y, max x, max y.
    pub bbox: [f32; 4],
    /// The item's own shapes and labels in [`Scene::prims`] (not its connectors).
    pub prims: std::ops::Range<usize>,
}

#[derive(Debug, Clone, Default)]
pub struct Scene {
    pub width: f32,
    pub height: f32,
    /// Drawn in order; every path is stroked with the pen.
    pub prims: Vec<Prim>,
    pub items: Vec<Item>,
}

#[derive(Debug, Clone)]
pub struct SceneOptions {
    /// Tree name for the header; no header when empty.
    pub title: String,
    /// Show primary event probabilities.
    pub probabilities: bool,
    /// Fill colours and monochrome mode (Edit > Preferences).
    pub prefs: crate::prefs::Preferences,
}

impl Default for SceneOptions {
    fn default() -> Self {
        Self {
            title: String::new(),
            probabilities: true,
            prefs: crate::prefs::Preferences::default(),
        }
    }
}

impl Scene {
    /// The item whose symbol contains `p`, if any.
    pub fn item_at(&self, p: [f32; 2]) -> Option<&Item> {
        self.items.iter().rev().find(|it| {
            let b = it.bbox;
            p[0] >= b[0] && p[0] <= b[2] && p[1] >= b[1] && p[1] <= b[3]
        })
    }
}

/// Symbol geometry relative to the top centre, in layout units (OpenFTA's
/// `symbol_defs.h`).
struct Symbol {
    width: f32,
    depth: f32,
    /// Area for the description, and how many lines fit.
    text_w: f32,
    text_lines: usize,
}

fn symbol(kind: EventType) -> Symbol {
    use EventType::*;
    let (width, depth, text_w, text_lines) = match kind {
        Basic | Dormant => (1.0, 1.0, 0.7, 6),
        Undeveloped => (1.3, 0.8, 0.75, 3),
        Intermediate | Unknown => (1.04, 0.70, 0.97, 5),
        And | PriorityAnd => (0.4, 0.4, 0.0, 0),
        Or | Xor => (0.4, 0.4, 0.0, 0),
        TransferIn => (0.42, 0.51, 0.0, 0),
        TransferOut => (0.42, 0.33, 0.0, 0),
        ConditionAnalysed | ConditionNotAnalysed => (1.04, 0.64, 1.04, 2),
        External => (0.84, 1.0, 0.80, 10),
        Inhibit => (0.4, 0.52, 0.0, 0),
    };
    Symbol {
        width,
        depth,
        text_w,
        text_lines,
    }
}

fn is_dormant(n: &Node) -> bool {
    n.kind == EventType::Dormant || (n.kind.is_primary() && n.modifier == Some('D'))
}

/// Conditions and transfer-out symbols are drawn beside their parent.
fn is_side(n: &Node) -> bool {
    matches!(
        n.kind,
        EventType::ConditionAnalysed | EventType::ConditionNotAnalysed | EventType::TransferOut
    )
}

/// Points of the arc of the ellipse centred on (cx, cy) from angle `a0` to
/// `a1` (degrees, counter-clockwise from 3 o'clock as on screen, y down).
fn arc(cx: f32, cy: f32, rx: f32, ry: f32, a0: f32, a1: f32) -> Vec<[f32; 2]> {
    let steps = (((a1 - a0).abs() / 360.0) * ELLIPSE_STEPS as f32)
        .ceil()
        .max(2.0) as usize;
    (0..=steps)
        .map(|i| {
            let a = (a0 + (a1 - a0) * i as f32 / steps as f32).to_radians();
            [cx + rx * a.cos(), cy - ry * a.sin()]
        })
        .collect()
}

/// The outline(s) of a symbol in layout units relative to its top centre:
/// (points, closed, filled).
fn outlines(kind: EventType) -> Vec<Outline> {
    use EventType::*;
    let dome = || {
        // AND: half ellipse over a short rectangle
        let mut p = arc(0.0, 0.28, 0.2, 0.28, 180.0, 0.0);
        p.extend([[0.2, 0.4], [-0.2, 0.4]]);
        p
    };
    let arch = || {
        // OR: tall half ellipse with a shallow curved base cut out
        let mut p = arc(0.0, 0.4, 0.2, 0.4, 180.0, 0.0);
        let base = arc(0.0, 0.4, 0.2, 0.05, 0.0, 180.0);
        // the base shares its end points with the arch
        p.extend(&base[1..base.len() - 1]);
        p
    };
    let chevron = vec![[-0.2, 0.4], [0.0, 0.0], [0.2, 0.4]];
    // a closed ellipse without its repeated first point
    let ellipse = |cx, cy, rx, ry| {
        let mut p = arc(cx, cy, rx, ry, 0.0, 360.0);
        p.pop();
        p
    };
    match kind {
        Basic | Dormant => vec![(ellipse(0.0, 0.5, 0.5, 0.5), true, true)],
        Undeveloped => vec![(
            vec![[0.0, 0.0], [-0.65, 0.4], [0.0, 0.8], [0.65, 0.4]],
            true,
            true,
        )],
        Intermediate | Unknown => vec![(
            vec![[-0.52, 0.0], [0.52, 0.0], [0.52, 0.7], [-0.52, 0.7]],
            true,
            true,
        )],
        And => vec![(dome(), true, true)],
        PriorityAnd => vec![(dome(), true, true), (chevron, false, false)],
        Or => vec![(arch(), true, true)],
        Xor => vec![(arch(), true, true), (chevron, false, false)],
        TransferIn => vec![
            (vec![[0.0, 0.18], [-0.21, 0.51], [0.21, 0.51]], true, true),
            (vec![[0.0, 0.0], [0.0, 0.18]], false, false),
        ],
        TransferOut => vec![
            (vec![[0.0, 0.0], [0.21, 0.33], [-0.21, 0.33]], true, true),
            (vec![[-0.06, 0.09], [-0.21, 0.09]], false, false),
        ],
        ConditionAnalysed | ConditionNotAnalysed => {
            vec![(ellipse(0.0, 0.32, 0.52, 0.32), true, true)]
        }
        External => vec![(
            vec![
                [0.0, 0.0],
                [-0.42, 0.12],
                [-0.42, 1.0],
                [0.42, 1.0],
                [0.42, 0.12],
            ],
            true,
            true,
        )],
        Inhibit => vec![(
            vec![
                [0.0, 0.0],
                [-0.2, 0.12],
                [-0.2, 0.4],
                [0.0, 0.52],
                [0.2, 0.4],
                [0.2, 0.12],
            ],
            true,
            true,
        )],
    }
}

/// One shape of a symbol: its points, whether it is closed and whether it is filled.
pub type Outline = (Vec<[f32; 2]>, bool, bool);

/// A symbol's outline as drawn in the tree, for palettes and icons: the shapes
/// (points, closed, filled) in layout units from the symbol's top centre, and
/// its size (width, depth).
pub fn symbol_outline(kind: EventType) -> (Vec<Outline>, [f32; 2]) {
    let s = symbol(kind);
    (outlines(kind), [s.width, s.depth])
}

/// Split `text` into lines of at most `width` characters, at most `max_lines`
/// of them; the last one ends with "..." when text was cut, as OpenFTA does.
pub fn wrap(text: &str, width: usize, max_lines: usize) -> Vec<String> {
    let width = width.max(4);
    let mut lines: Vec<String> = Vec::new();
    let mut cur = String::new();
    for word in text.split_whitespace() {
        let mut word: Vec<char> = word.chars().collect();
        loop {
            let clen = cur.chars().count();
            let room = if clen == 0 {
                width
            } else {
                width.saturating_sub(clen + 1)
            };
            if word.len() <= room || (clen == 0 && word.len() <= width) {
                if clen > 0 {
                    cur.push(' ');
                }
                cur.extend(word.iter());
                break;
            }
            if clen > 0 {
                lines.push(std::mem::take(&mut cur));
                continue;
            }
            // a word longer than a line is split
            cur.extend(word.drain(..width));
            lines.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    if lines.len() > max_lines {
        lines.truncate(max_lines);
        if let Some(last) = lines.last_mut() {
            let keep = width.saturating_sub(3);
            *last = last.chars().take(keep).collect::<String>() + "...";
        }
    }
    lines
}

/// `value` formatted like C's `%g`: six significant digits, no trailing zeros.
pub fn format_g(value: f64) -> String {
    if value == 0.0 || !value.is_finite() {
        return if value.is_finite() {
            "0".into()
        } else {
            value.to_string()
        };
    }
    let exp = value.abs().log10().floor() as i32;
    let trim = |s: String| -> String {
        if s.contains('.') {
            s.trim_end_matches('0').trim_end_matches('.').to_string()
        } else {
            s
        }
    };
    if !(-4..6).contains(&exp) {
        let s = format!("{value:.5e}");
        let (m, e) = s.split_once('e').unwrap_or((&s, "0"));
        let e: i32 = e.parse().unwrap_or(0);
        format!(
            "{}e{}{:02}",
            trim(m.to_string()),
            if e < 0 { '-' } else { '+' },
            e.abs()
        )
    } else {
        let decimals = (5 - exp).max(0) as usize;
        trim(format!("{value:.decimals$}"))
    }
}

struct Placed<'a> {
    node: &'a Node,
    path: Vec<usize>,
    /// Centre x and top y, in pixels.
    x: f32,
    top: f32,
    parent: Option<usize>,
    side: bool,
}

fn place<'a>(
    n: &'a Node,
    row: usize,
    path: Vec<usize>,
    parent: Option<usize>,
    next_col: &mut f32,
    out: &mut Vec<Placed<'a>>,
) -> usize {
    let me = out.len();
    out.push(Placed {
        node: n,
        path: path.clone(),
        x: 0.0,
        top: row as f32 * ROW,
        parent,
        side: false,
    });
    let mut cols = Vec::new();
    let mut side = None;
    for (i, c) in n.children.iter().enumerate() {
        let mut p = path.clone();
        p.push(i);
        if i == 0 && is_side(c) {
            side = Some((c, p));
        } else {
            cols.push(place(c, row + 1, p, Some(me), next_col, out));
        }
    }
    let col = if let (Some(&a), Some(&b)) = (cols.first(), cols.last()) {
        (out[a].x / COL + out[b].x / COL) / 2.0
    } else {
        let c = *next_col;
        *next_col += 1.0;
        c
    };
    out[me].x = col * COL;
    if let Some((c, p)) = side {
        // beside the parent, one column to the right, kept clear of its neighbours
        *next_col = next_col.max(col + 2.0);
        let parent_sym = symbol(n.kind);
        let own = symbol(c.kind);
        let mid = out[me].top + parent_sym.depth * UNIT / 2.0;
        let top = if c.kind == EventType::TransferOut {
            mid - 0.09 * UNIT
        } else {
            mid - own.depth * UNIT / 2.0
        };
        out.push(Placed {
            node: c,
            path: p,
            x: (col + 1.0) * COL,
            top,
            parent: Some(me),
            side: true,
        });
    }
    me
}

struct Builder {
    prims: Vec<Prim>,
    min: [f32; 2],
    max: [f32; 2],
}

impl Builder {
    fn grow(&mut self, x: f32, y: f32) {
        self.min = [self.min[0].min(x), self.min[1].min(y)];
        self.max = [self.max[0].max(x), self.max[1].max(y)];
    }
    fn path(&mut self, pts: Vec<[f32; 2]>, closed: bool, fill: Option<Rgb>, dashed: bool) {
        for p in &pts {
            self.grow(p[0], p[1]);
        }
        self.prims.push(Prim::Path {
            pts,
            closed,
            fill,
            dashed,
        });
    }
    fn line(&mut self, a: [f32; 2], b: [f32; 2]) {
        self.path(vec![a, b], false, None, false);
    }
    fn text(&mut self, pos: [f32; 2], size: f32, anchor: Anchor, bold: bool, text: &str) {
        if text.is_empty() {
            return;
        }
        let w = text.chars().count() as f32 * size * CHAR_W;
        let x0 = match anchor {
            Anchor::Start => pos[0],
            Anchor::Middle => pos[0] - w / 2.0,
        };
        self.grow(x0, pos[1] - size * 0.6);
        self.grow(x0 + w, pos[1] + size * 0.6);
        self.prims.push(Prim::Text {
            pos,
            size,
            anchor,
            bold,
            text: text.to_string(),
        });
    }
}

/// The description drawn inside an item's symbol, if any.
fn description<'a>(n: &'a Node, db: &'a PrimaryEventDb) -> &'a str {
    use EventType::*;
    match n.kind {
        k if k.is_gate() => "",
        k if k.is_primary() => db.events.get(&n.id).map_or("", |e| e.description.as_str()),
        Intermediate | ConditionAnalysed | ConditionNotAnalysed | Unknown => {
            if n.text.is_empty() {
                db.events.get(&n.id).map_or("", |e| e.description.as_str())
            } else {
                &n.text
            }
        }
        _ => "",
    }
}

/// Lay out and draw `tree`. `db` supplies primary event descriptions and
/// probabilities.
pub fn tree_scene(tree: &Tree, db: &PrimaryEventDb, opts: &SceneOptions) -> Scene {
    let mut placed = Vec::new();
    let mut next_col = 0.0;
    place(&tree.root, 0, Vec::new(), None, &mut next_col, &mut placed);

    let mut b = Builder {
        prims: Vec::new(),
        min: [f32::INFINITY; 2],
        max: [f32::NEG_INFINITY; 2],
    };

    // connectors first, so symbols sit on top of them
    draw_connectors(&mut b, &placed);

    let mut items: Vec<Item> = placed
        .iter()
        .map(|p| draw_node(&mut b, p, db, opts))
        .collect();

    let header = header_lines(tree, opts);
    let header_h = if header.is_empty() {
        0.0
    } else {
        header.len() as f32 * HEADER_LINE_H + HEADER_LINE_H
    };

    if b.prims.is_empty() {
        b.grow(0.0, 0.0);
    }
    // move everything so the drawing starts at the margin, under the header
    let dx = MARGIN - b.min[0];
    let dy = MARGIN + header_h - b.min[1];
    shift(&mut b, &mut items, dx, dy);
    let mut width = b.max[0] - b.min[0] + 2.0 * MARGIN;
    let height = b.max[1] - b.min[1] + 2.0 * MARGIN + header_h;
    for (i, l) in header.iter().enumerate() {
        let pos = [MARGIN, MARGIN + (i as f32 + 0.5) * HEADER_LINE_H];
        width = width.max(MARGIN * 2.0 + l.chars().count() as f32 * HEADER_FONT * CHAR_W);
        b.prims.push(Prim::Text {
            pos,
            size: HEADER_FONT,
            anchor: Anchor::Start,
            bold: i == 0,
            text: l.clone(),
        });
    }
    Scene {
        width,
        height,
        prims: b.prims,
        items,
    }
}

/// The lines joining each event to its parent and children (side events: across).
fn draw_connectors(b: &mut Builder, placed: &[Placed]) {
    for (i, p) in placed.iter().enumerate() {
        let sym = symbol(p.node.kind);
        let dormant_lift = if is_dormant(p.node) {
            sym.depth * UNIT * 0.05
        } else {
            0.0
        };
        if p.side {
            let parent = &placed[p.parent.unwrap_or(0)];
            let ps = symbol(parent.node.kind);
            let y = parent.top + ps.depth * UNIT / 2.0;
            let from = parent.x + ps.width * UNIT / 2.0;
            let to = if p.node.kind == EventType::TransferOut {
                p.x - 0.21 * UNIT
            } else {
                p.x - sym.width * UNIT / 2.0
            };
            b.line([from, y], [to, y]);
            continue;
        }
        if p.parent.is_some() {
            b.line([p.x, p.top - dormant_lift], [p.x, p.top - CONNECT]);
        }
        let kids: Vec<&Placed> = placed
            .iter()
            .filter(|c| c.parent == Some(i) && !c.side)
            .collect();
        if let (Some(first), Some(last)) = (kids.first(), kids.last()) {
            let bar = p.top + ROW - CONNECT;
            b.line([p.x, p.top + sym.depth * UNIT], [p.x, bar]);
            if kids.len() > 1 {
                b.line([first.x, bar], [last.x, bar]);
            }
        }
    }
}

/// One event: its symbol, id, probability and description; returns its hit box.
fn draw_node(b: &mut Builder, p: &Placed, db: &PrimaryEventDb, opts: &SceneOptions) -> Item {
    let n = p.node;
    let sym = symbol(n.kind);
    let start = b.prims.len();
    let at = |q: [f32; 2]| [p.x + q[0] * UNIT, p.top + q[1] * UNIT];
    let f = {
        let c = opts.prefs.fill(n.kind);
        Rgb(c[0], c[1], c[2])
    };
    if is_dormant(n) {
        // a ring 1.1 times the size behind the circle
        let r = 0.55 * UNIT;
        let c = [p.x, p.top + 0.5 * UNIT];
        let mut ring = arc(c[0], c[1], r, r, 0.0, 360.0);
        ring.pop();
        b.path(ring, true, Some(f), false);
    }
    let dashed = n.kind == EventType::ConditionNotAnalysed;
    for (pts, closed, filled) in outlines(n.kind) {
        let pts = pts.into_iter().map(at).collect();
        b.path(pts, closed, filled.then_some(f), dashed);
    }

    // id above the symbol, right of the connector (`draw_id`)
    let id_y = p.top - 1.35 * FONT;
    b.text([p.x + FONT / 2.0, id_y], FONT, Anchor::Start, false, &n.id);
    if opts.probabilities && n.kind.is_primary() {
        if let Some(e) = db.events.get(&n.id) {
            b.text(
                [p.x + 1.5 * FONT, p.top - 0.3 * FONT],
                FONT,
                Anchor::Start,
                false,
                &format_g(e.probability),
            );
        }
    }
    // description inside the symbol, centred (`draw_symbol_text`)
    let desc = description(n, db);
    if sym.text_lines > 0 && !desc.is_empty() {
        // a little wider than the average glyph, so lines stay inside the symbol
        let chars = (sym.text_w * UNIT / (FONT * 0.58)).floor() as usize;
        let lines = wrap(desc, chars, sym.text_lines);
        let h = lines.len() as f32 * LINE_H;
        let mut y = p.top + (sym.depth * UNIT - h) / 2.0 + LINE_H / 2.0;
        for l in &lines {
            b.text([p.x, y], FONT, Anchor::Middle, false, l);
            y += LINE_H;
        }
    }
    let half = sym.width * UNIT / 2.0;
    let lift = if is_dormant(n) { 0.05 * UNIT } else { 0.0 };
    Item {
        path: p.path.clone(),
        bbox: [
            p.x - half - lift,
            p.top - lift,
            p.x + half + lift,
            p.top + sym.depth * UNIT + lift,
        ],
        prims: start..b.prims.len(),
    }
}

/// OpenFTA's header in the top left corner: tree, database and note; none without a title.
fn header_lines(tree: &Tree, opts: &SceneOptions) -> Vec<String> {
    let mut header: Vec<String> = Vec::new();
    if !opts.title.is_empty() {
        header.push(format!("Tree: {}", opts.title));
        if let Some(db) = tree.db_name.as_deref().filter(|n| !n.is_empty()) {
            header.push(format!("Database: {db}"));
        }
        header.extend(wrap(&tree.note, 48, 4));
    }
    header
}

/// Move every primitive and hit box by (dx, dy).
fn shift(b: &mut Builder, items: &mut [Item], dx: f32, dy: f32) {
    for prim in &mut b.prims {
        match prim {
            Prim::Path { pts, .. } => {
                for q in pts {
                    q[0] += dx;
                    q[1] += dy;
                }
            }
            Prim::Text { pos, .. } => {
                pos[0] += dx;
                pos[1] += dy;
            }
        }
    }
    for it in items.iter_mut() {
        it.bbox = [
            it.bbox[0] + dx,
            it.bbox[1] + dy,
            it.bbox[2] + dx,
            it.bbox[3] + dy,
        ];
    }
}

/// Triangles that fill the simple polygon `pts` (ear clipping), as index
/// triples. Works for the concave OR gate outline as well as convex shapes.
pub fn triangulate(pts: &[[f32; 2]]) -> Vec<[usize; 3]> {
    let n = pts.len();
    if n < 3 {
        return Vec::new();
    }
    let area: f32 = (0..n)
        .map(|i| {
            let (a, b) = (pts[i], pts[(i + 1) % n]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum();
    let sign = if area >= 0.0 { 1.0 } else { -1.0 };
    let cross = |a: [f32; 2], b: [f32; 2], c: [f32; 2]| {
        ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])) * sign
    };
    // points on an edge count as inside, so an ear never swallows a notch
    let inside = |p: [f32; 2], a, b, c| {
        cross(a, b, p) >= 0.0 && cross(b, c, p) >= 0.0 && cross(c, a, p) >= 0.0
    };
    let mut idx: Vec<usize> = (0..n).collect();
    let mut out = Vec::with_capacity(n - 2);
    let mut guard = 0;
    while idx.len() > 3 && guard < n * n {
        guard += 1;
        let m = idx.len();
        let mut clipped = false;
        for k in 0..m {
            let (ia, ib, ic) = (idx[(k + m - 1) % m], idx[k], idx[(k + 1) % m]);
            let (a, b, c) = (pts[ia], pts[ib], pts[ic]);
            if cross(a, b, c) <= 0.0 {
                continue; // reflex or degenerate corner
            }
            if idx
                .iter()
                .filter(|&&j| j != ia && j != ib && j != ic)
                .any(|&j| inside(pts[j], a, b, c))
            {
                continue;
            }
            out.push([ia, ib, ic]);
            idx.remove(k);
            clipped = true;
            break;
        }
        if !clipped {
            // degenerate input: drop a collinear point and go on
            idx.remove(0);
        }
    }
    if idx.len() == 3 {
        out.push([idx[0], idx[1], idx[2]]);
    }
    out
}
