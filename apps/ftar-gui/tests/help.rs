//! Help menu and user guide window (headless: egui_kittest, no window or GPU).

use eframe::egui::vec2;
use egui_kittest::{
    kittest::{NodeT, Queryable},
    Harness,
};
use ftar_gui::{App, HelpState};

fn harness() -> Harness<'static, App> {
    Harness::builder()
        .with_size(vec2(1280.0, 800.0))
        .build_eframe(|_| App::default())
}

fn topic_index(state: &HelpState, title: &str) -> Option<usize> {
    state.topics.iter().position(|t| t.title == title)
}

#[test]
fn help_menu_opens_the_user_guide_on_the_overview() {
    let mut h = harness();
    h.run();
    assert!(!h.state().help.open, "help starts closed");
    h.get_by_label("Help").click();
    h.run();
    h.get_by_label("User guide").click();
    h.run();
    let state = &h.state().help;
    assert!(state.open);
    assert_eq!(state.topic, 0, "user guide starts on the overview");
    h.get_by_label("Getting started");
    h.get_by_label("Editing the tree");
}

#[test]
fn about_entry_opens_the_licence_page() {
    let mut h = harness();
    h.run();
    h.get_by_label("Help").click();
    h.run();
    h.get_by_label("About ftar").click();
    h.run();
    let idx = topic_index(&h.state().help, "About ftar").expect("guide has an about page");
    assert_eq!(h.state().help.topic, idx);
    h.get_by_label_contains("GNU General Public Licence");
}

#[test]
fn clicking_a_topic_shows_its_page() {
    let mut h = harness();
    h.run();
    h.get_by_label("Help").click();
    h.run();
    h.get_by_label("User guide").click();
    h.run();
    h.get_by_label("Saving").click();
    h.run();
    let idx = topic_index(&h.state().help, "Saving").expect("guide has a saving page");
    assert_eq!(h.state().help.topic, idx);
    h.get_by_label_contains("Ctrl+S");
}

#[test]
fn help_is_available_without_a_tree_and_help_menu_is_always_enabled() {
    let mut h = harness();
    h.run();
    h.get_by_label("Help").click();
    h.run();
    assert!(!h.get_by_label("User guide").accesskit_node().is_disabled());
    assert!(!h.get_by_label("About ftar").accesskit_node().is_disabled());
}
