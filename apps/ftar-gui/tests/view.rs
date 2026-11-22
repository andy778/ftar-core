//! View menu: zoom commands and the Tree Overview (#54), driven headlessly.

use std::path::PathBuf;

use eframe::egui::{self, vec2};
use egui_kittest::{
    kittest::{NodeT, Queryable},
    Harness,
};
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
fn view_menu_zooms_in_and_out() {
    let mut h = harness();
    h.run();
    let z0 = h.state().zoom;
    h.get_by_label("View").click();
    h.run();
    h.get_by_label_contains("Zoom In").click();
    h.run();
    assert!(h.state().zoom > z0 * 1.2 && !h.state().auto_fit);
    h.get_by_label("View").click();
    h.run();
    h.get_by_label_contains("Zoom Out").click();
    h.run();
    assert!((h.state().zoom - z0).abs() < 1e-4);
}

#[test]
fn zoom_shortcuts_work() {
    let mut h = harness();
    h.run();
    let z0 = h.state().zoom;
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Equals);
    h.run();
    assert!(h.state().zoom > z0);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Minus);
    h.run();
    assert!((h.state().zoom - z0).abs() < 1e-4);
}

#[test]
fn set_zoom_dialog_sets_a_percentage() {
    let mut h = harness();
    h.run();
    h.get_by_label("View").click();
    h.run();
    h.get_by_label_contains("Set Zoom").click();
    h.run();
    h.get_by_label("Zoom (%)");
    h.state_mut().zoom_dialog = Some(50.0);
    h.run();
    h.get_by_label("OK").click();
    h.run();
    assert!((h.state().zoom - 0.5).abs() < 1e-4);
    assert!(h.state().zoom_dialog.is_none());
    // out-of-range values are clamped, never rejected
    h.state_mut().set_zoom_percent(100_000.0);
    assert_eq!(h.state().zoom, 4.0);
    h.state_mut().set_zoom_percent(f32::NAN);
    assert_eq!(h.state().zoom, 4.0);
}

#[test]
fn tree_overview_opens_and_a_click_moves_the_view() {
    let mut h = harness();
    h.run();
    h.get_by_label("View").click();
    h.run();
    h.get_by_label("Tree Overview").click();
    h.run();
    assert!(h.state().overview_open);
    h.state_mut().zoom_in();
    h.run();
    let before = h.state().pan;
    let win = h.get_by_label("Tree Overview").rect();
    let p = egui::pos2(win.min.x + 30.0, win.max.y - 40.0);
    h.hover_at(p);
    h.run();
    h.event(egui::Event::PointerButton {
        pos: p,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: Default::default(),
    });
    h.event(egui::Event::PointerButton {
        pos: p,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: Default::default(),
    });
    h.run();
    assert_ne!(h.state().pan, before);
    assert!(!h.state().auto_fit);
}

#[test]
fn view_menu_is_disabled_without_a_tree() {
    let mut h = Harness::builder()
        .with_size(vec2(1280.0, 800.0))
        .build_eframe(|_| App::default());
    h.run();
    h.get_by_label("View").click();
    h.run();
    assert!(h
        .get_by_label_contains("Zoom In")
        .accesskit_node()
        .is_disabled());
}
