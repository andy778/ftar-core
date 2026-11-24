//! The canvas fills the drawing area edge to edge and carries scrollbars (#97).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use eframe::egui::vec2;
use egui_kittest::Harness;
use ftar_gui::App;

fn harness() -> Harness<'static, App> {
    let mut app = App::default();
    app.open(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../crates/core/tests/data/theatre.fta"),
    );
    Harness::builder()
        .with_size(vec2(1280.0, 800.0))
        .build_eframe(move |_| app)
}

#[test]
fn fitted_tree_needs_no_scrollbar_but_zoomed_tree_does() {
    let mut h = harness();
    h.run_steps(8);
    let view = h.state().canvas.size();
    assert!(
        (h.state().scroll_extent - view).abs().max_elem() < 0.01,
        "{:?} vs {view:?}",
        h.state().scroll_extent
    );
    for _ in 0..4 {
        h.state_mut().zoom_in();
    }
    h.run_steps(8);
    let e = h.state().scroll_extent;
    assert!(
        e.x > view.x || e.y > view.y,
        "zoomed in: scrollable, {e:?} vs {view:?}"
    );
}

#[test]
fn scroll_extent_follows_zoom_and_fit_restores_it() {
    let mut h = harness();
    h.run_steps(8);
    for _ in 0..4 {
        h.state_mut().zoom_in();
    }
    h.run_steps(8);
    h.state_mut().fit_next = true;
    h.run_steps(8);
    let view = h.state().canvas.size();
    assert!(
        (h.state().scroll_extent - view).abs().max_elem() < 0.01,
        "{:?} vs {view:?}",
        h.state().scroll_extent
    );
}

#[test]
fn zooming_in_keeps_the_point_under_the_centre_and_hit_testing_works() {
    let mut h = harness();
    h.run_steps(8);
    let centre = h.state().canvas.center();
    let world =
        (centre.to_vec2() - h.state().canvas.min.to_vec2() - h.state().pan) / h.state().zoom;
    h.state_mut().zoom_in();
    h.run_steps(8);
    let after =
        (centre.to_vec2() - h.state().canvas.min.to_vec2() - h.state().pan) / h.state().zoom;
    assert!((world - after).length() < 0.5, "{world:?} vs {after:?}");
    assert!(h.state().content_bounds().is_some());
}
