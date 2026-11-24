//! Page setup and print preview (issue #56).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use eframe::egui::vec2;
use egui_kittest::{
    kittest::{NodeT, Queryable},
    Harness,
};
use ftar_gui::App;
use ftar_render::{Fit, Orientation, PageSize};

fn data(name: &str) -> PathBuf {
    PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../crates/core/tests/data"
    ))
    .join(name)
}

fn loaded() -> App {
    let mut app = App::default();
    app.open(data("theatre.fta"));
    app
}

#[test]
fn setup_shapes_the_print_page_and_the_preview_sheets() {
    let mut app = loaded();
    app.preview.setup.size = PageSize::Letter;
    app.preview.setup.orientation = Orientation::Portrait;
    let html = app.print_page(false).unwrap();
    assert!(html.contains("size: 215.9mm 279.4mm"), "page size in CSS");
    assert_eq!(app.preview_sheets().unwrap().len(), 1);

    app.preview.setup.fit = Fit::SplitPages;
    app.preview.setup.size = PageSize::A5;
    app.preview.setup.scale_percent = 200.0;
    let sheets = app.preview_sheets().unwrap();
    assert!(sheets.len() > 1, "a big drawing needs several A5 sheets");
    let (n, w, h, px) = app.preview_pixels(1).unwrap();
    assert_eq!(n, sheets.len());
    assert_eq!(px.len(), (w * h * 4) as usize);
    assert!(px.iter().any(|&b| b != 255), "the sheet shows something");
    // out-of-range sheet clamps instead of panicking
    assert!(app.preview_pixels(10_000).is_ok());
    assert!(App::default().preview_sheets().is_err());
}

#[test]
fn preview_window_opens_from_the_file_menu_and_pages_through_sheets() {
    let mut app = loaded();
    app.preview.setup.fit = Fit::SplitPages;
    app.preview.setup.size = PageSize::A5;
    app.preview.setup.scale_percent = 200.0;
    let n = app.preview_sheets().unwrap().len();
    let mut h = Harness::builder()
        .with_size(vec2(1280.0, 900.0))
        .build_eframe(move |_| app);
    h.run();
    h.get_by_label("File").click();
    h.run();
    h.get_by_label("Page setup and print preview...").click();
    h.run();
    h.get_by_label("Page setup and print preview");
    h.get_by_label(&format!("Page 1 of {n}"));
    for name in ["Next page", "Previous page", "Print..."] {
        h.get_all_by_label(name).last().unwrap(); // "Print..." is also on the toolbar
    }
    h.get_by_label("Next page").click();
    h.run();
    assert_eq!(h.state().preview.sheet, 1);
    h.get_by_label("Print preview of the selected page");
}

#[test]
fn menu_item_is_disabled_without_a_tree() {
    let mut h = Harness::builder()
        .with_size(vec2(1280.0, 800.0))
        .build_eframe(|_| App::default());
    h.run();
    h.get_by_label("File").click();
    h.run();
    assert!(h
        .get_by_label("Page setup and print preview...")
        .accesskit_node()
        .is_disabled());
}
