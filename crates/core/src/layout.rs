//! Tidy top-down tree layout: leaves get consecutive columns, parents are
//! centred over their children.

use crate::model::Node;

pub struct Placed<'a> {
    pub node: &'a Node,
    /// Column (may be fractional) and row.
    pub x: f32,
    pub y: f32,
    pub parent: Option<usize>,
    /// Child indices from the root.
    pub path: Vec<usize>,
}

pub fn layout(root: &Node) -> Vec<Placed<'_>> {
    let mut out = Vec::new();
    let mut next_col = 0.0;
    place(root, 0, None, Vec::new(), &mut next_col, &mut out);
    out
}

fn place<'a>(
    n: &'a Node,
    depth: usize,
    parent: Option<usize>,
    path: Vec<usize>,
    next_col: &mut f32,
    out: &mut Vec<Placed<'a>>,
) -> usize {
    let me = out.len();
    out.push(Placed {
        node: n,
        x: 0.0,
        y: depth as f32,
        parent,
        path: path.clone(),
    });
    if n.children.is_empty() {
        out[me].x = *next_col;
        *next_col += 1.0;
    } else {
        let ids: Vec<usize> = n
            .children
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let mut p = path.clone();
                p.push(i);
                place(c, depth + 1, Some(me), p, next_col, out)
            })
            .collect();
        if let (Some(&first), Some(&last)) = (ids.first(), ids.last()) {
            out[me].x = (out[first].x + out[last].x) / 2.0;
        }
    }
    me
}
