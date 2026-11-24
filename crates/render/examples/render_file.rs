//! `cargo run -p ftar-render --example render_file -- tree.fta out.png [scale]`
//! Renders a tree to PNG (or SVG when `out` ends in .svg) - handy for looking at the drawing.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use ftar_core::project::Project;
use ftar_render::{svg_to_png, tree_svg, SvgOptions};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let p = Project::load(&a[1]).expect("load");
    let svg = tree_svg(
        &p.tree,
        &p.db,
        &SvgOptions {
            title: "Fault tree".into(),
            ..Default::default()
        },
    );
    if a[2].ends_with(".svg") {
        std::fs::write(&a[2], svg).unwrap();
    } else {
        let scale = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(1.0);
        std::fs::write(&a[2], svg_to_png(&svg, scale).expect("png")).unwrap();
    }
}
