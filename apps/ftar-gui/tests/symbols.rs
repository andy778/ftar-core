//! The canvas draws OpenFTA's symbols (issue #52). Headless: no window, no GPU.

use std::path::PathBuf;

use eframe::egui::{vec2, Visuals};
use egui_kittest::Harness;
use ftar_core::edit::new_node;
use ftar_core::EventType;
use ftar_gui::App;

fn theatre() -> PathBuf {
    PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../crates/core/tests/data/theatre.fta"
    ))
}

#[test]
fn canvas_draws_every_symbol_kind_and_stays_fitted() {
    let mut app = App::default();
    app.open(theatre());
    let doc = app.doc.as_mut().expect("tree loaded");
    for k in [
        EventType::Basic,
        EventType::Undeveloped,
        EventType::External,
        EventType::Dormant,
        EventType::ConditionAnalysed,
        EventType::ConditionNotAnalysed,
        EventType::TransferIn,
        EventType::TransferOut,
        EventType::Intermediate,
        EventType::Xor,
        EventType::PriorityAnd,
        EventType::Inhibit,
    ] {
        let mut n = new_node(k, &format!("{k:?}"));
        n.text = "A long description of this item that has to be wrapped inside".into();
        doc.tree.root.children[0].children.push(n);
    }
    let mut h = Harness::builder()
        .with_size(vec2(1280.0, 800.0))
        .build_eframe(move |_| app);
    h.run();
    for dark in [false, true] {
        h.ctx.set_visuals(if dark {
            Visuals::dark()
        } else {
            Visuals::light()
        });
        h.run();
        let (canvas, b) = (
            h.state().canvas,
            h.state().content_bounds().expect("bounds"),
        );
        assert!(
            canvas.contains_rect(b),
            "dark={dark}: {b:?} not in {canvas:?}"
        );
    }
}
