//! The icon toolbar and the symbol palette (#72).

use std::path::PathBuf;

use eframe::egui::vec2;
use egui_kittest::{
    kittest::{NodeT, Queryable},
    Harness,
};
use ftar_core::edit::node_at;
use ftar_core::EventType;
use ftar_gui::App;

const DATA: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../crates/core/tests/data");

fn harness_for(app: App) -> Harness<'static, App> {
    Harness::builder()
        .with_size(vec2(1280.0, 800.0))
        .build_eframe(move |_| app)
}

fn theatre() -> App {
    let mut app = App::default();
    app.open(PathBuf::from(DATA).join("theatre.fta"));
    app
}

/// The toolbar button (first in the tree; the edit panel has its own "Delete").
fn disabled(h: &Harness<'_, App>, label: &str) -> bool {
    h.get_all_by_label(label)
        .next()
        .unwrap()
        .accesskit_node()
        .is_disabled()
}

#[test]
fn toolbar_has_openftas_commands_and_enables_them_like_the_menus() {
    let mut h = harness_for(App::default());
    h.run();
    // without a tree only New and Open work
    for on in ["New", "Open..."] {
        assert!(!disabled(&h, on), "{on}");
    }
    for off in [
        "Save", "Print...", "Cut", "Copy", "Paste", "Delete", "Link", "Undo", "Redo", "Zoom In",
        "Zoom Out", "Fit",
    ] {
        assert!(disabled(&h, off), "{off}");
    }
    let mut h = harness_for(theatre());
    h.run();
    for on in ["Save", "Print...", "Zoom In", "Zoom Out", "Fit"] {
        assert!(!disabled(&h, on), "{on}");
    }
    assert!(
        disabled(&h, "Cut") && disabled(&h, "Copy"),
        "nothing selected"
    );
    h.state_mut().select(Some(vec![0, 1]));
    h.run();
    assert!(!disabled(&h, "Cut") && !disabled(&h, "Copy"));
    assert!(disabled(&h, "Paste"), "clipboard is empty");
    assert!(!disabled(&h, "Delete"));
    h.state_mut().select(Some(vec![]));
    h.run();
    assert!(disabled(&h, "Delete"), "the top event cannot be deleted");
}

#[test]
fn toolbar_delete_removes_the_selected_branch_and_can_be_undone() {
    let mut h = harness_for(theatre());
    h.run();
    h.state_mut().select(Some(vec![0, 1])); // MAINS_FAIL
    h.run();
    h.get_all_by_label("Delete").next().unwrap().click();
    h.run();
    assert_eq!(
        node_at(&h.state().doc.as_ref().unwrap().tree, &[0])
            .unwrap()
            .children
            .len(),
        1
    );
    assert!(h.state().dirty);
    h.get_by_label("Undo").click();
    h.run();
    assert_eq!(
        node_at(&h.state().doc.as_ref().unwrap().tree, &[0])
            .unwrap()
            .children
            .len(),
        2
    );
}

#[test]
fn toolbar_buttons_run_the_commands() {
    let mut h = harness_for(theatre());
    h.run();
    let z0 = h.state().zoom;
    h.get_by_label("Zoom In").click();
    h.run();
    assert!(h.state().zoom > z0);
    h.get_by_label("Zoom Out").click();
    h.run();
    assert!((h.state().zoom - z0).abs() < 1e-4);
    h.get_by_label("Fit").click();
    h.run();
    assert!(h.state().auto_fit);

    // copy and paste a branch, then undo it
    h.state_mut().select(Some(vec![0, 1]));
    h.run();
    h.get_by_label("Copy").click();
    h.run();
    assert!(h.state().clipboard.is_some());
    h.state_mut().select(Some(vec![0, 0, 0]));
    h.run();
    h.get_by_label("Paste").click();
    h.run();
    assert!(h.state().dirty);
    assert_eq!(
        node_at(&h.state().doc.as_ref().unwrap().tree, &[0, 0, 0])
            .unwrap()
            .children
            .len(),
        3
    );
    h.get_by_label("Undo").click();
    h.run();
    assert_eq!(
        node_at(&h.state().doc.as_ref().unwrap().tree, &[0, 0, 0])
            .unwrap()
            .children
            .len(),
        2
    );
    assert!(!disabled(&h, "Redo"));
}

#[test]
fn palette_adds_the_clicked_symbol_to_the_selection() {
    let mut h = harness_for(theatre());
    h.run();
    h.state_mut().select(Some(vec![0, 0, 0])); // the OR gate
    h.run();
    h.get_by_label("Add Undeveloped event").click();
    h.run();
    let app = h.state();
    assert_eq!(app.selected.as_deref(), Some(&[0usize, 0, 0, 2][..]));
    let n = node_at(&app.doc.as_ref().unwrap().tree, &[0, 0, 0, 2]).unwrap();
    assert_eq!(n.kind, EventType::Undeveloped);
    assert_eq!(
        app.add_kind,
        EventType::Undeveloped,
        "shown as the current symbol"
    );
    // a symbol the selection cannot take says why and changes nothing
    h.state_mut().select(Some(vec![0, 0, 0, 0])); // a basic event
    h.run();
    h.get_by_label("Add AND gate").click();
    h.run();
    assert!(h.state().error.is_some());
}

#[test]
fn palette_has_every_openfta_symbol_and_is_disabled_without_a_tree() {
    let names = [
        "Intermediate event",
        "Basic event",
        "Undeveloped event",
        "External event",
        "Condition (not analysed)",
        "AND gate",
        "OR gate",
        "Priority AND gate",
        "XOR gate",
        "Inhibit gate",
        "Transfer in",
        "Transfer out",
    ];
    let mut h = harness_for(App::default());
    h.run();
    for n in names {
        assert!(disabled(&h, &format!("Add {n}")), "{n}");
    }
    let mut h = harness_for(theatre());
    h.run();
    for n in names {
        assert!(!disabled(&h, &format!("Add {n}")), "{n}");
    }
}
