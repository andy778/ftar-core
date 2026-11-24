//! `cargo run -p ftar-render --example print_page -- tree.fta out.html` - writes the printable page.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use ftar_core::project::Project;
use ftar_render::{print_html, PrintOptions};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let p = Project::load(&a[1]).expect("load");
    let html = print_html(&p.tree, &p.db, &PrintOptions { title: "theatre".into(), probabilities: true, report: Some("Minimal Cut Sets\n=================\n\nOrder 2:\n  GEN_FAIL MAINS_FAIL\n  MAINS_FAIL RELAY_FAIL\n".into()), auto_print: false, ..Default::default() });
    std::fs::write(&a[2], html).unwrap();
}
