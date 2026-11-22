//! Edit > Selected... / double-click (#64) and Shift Left / Right (#65).

use std::path::PathBuf;

use eframe::egui::vec2;
use egui_kittest::{kittest::Queryable, Harness};
use ftar_core::edit::{append_child, new_node, node_at};
use ftar_core::EventType;
use ftar_gui::App;

const DATA: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../crates/core/tests/data");

fn theatre() -> App {
    let mut app = App::default();
    app.open(PathBuf::from(DATA).join("theatre.fta"));
    app
}

fn harness_for(app: App) -> Harness<'static, App> {
    Harness::builder()
        .with_size(vec2(1280.0, 800.0))
        .build_eframe(move |_| app)
}

fn id_at(app: &App, path: &[usize]) -> String {
    node_at(&app.doc.as_ref().unwrap().tree, path)
        .unwrap()
        .id
        .clone()
}

/// Top -> gate -> [condition C, basic A, basic B]
fn with_condition() -> App {
    let mut app = App::default();
    app.new_doc();
    let doc = app.doc.as_mut().unwrap();
    let t = &mut doc.tree;
    // a new tree already has a gate under the top event
    let g = vec![0usize];
    for (k, id) in [
        (EventType::ConditionNotAnalysed, "C"),
        (EventType::Basic, "A"),
        (EventType::Basic, "B"),
    ] {
        append_child(t, &g, new_node(k, id)).unwrap();
    }
    app
}

#[test]
fn shift_moves_one_place_and_is_silent_at_the_ends() {
    let mut app = theatre();
    // Top -> AND1 -> [Backup_Fail, MAINS_FAIL]
    app.select(Some(vec![0, 1]));
    app.move_selected(1);
    assert!(
        app.error.is_none() && !app.dirty,
        "last item: nothing happens, no error"
    );
    app.move_selected(-1);
    assert_eq!(app.selected.as_deref(), Some(&[0usize, 0][..]));
    assert_eq!(id_at(&app, &[0, 0]), "MAINS_FAIL");
    app.move_selected(-1);
    assert_eq!(
        id_at(&app, &[0, 0]),
        "MAINS_FAIL",
        "first item: nothing happens"
    );
    assert!(app.error.is_none());
    app.undo();
    assert_eq!(id_at(&app, &[0, 1]), "MAINS_FAIL");
    // nothing selected: a message, not a panic
    app.select(None);
    app.move_selected(1);
    assert!(app.error.is_some());
}

#[test]
fn conditions_are_not_shifted_and_nothing_passes_them() {
    let mut app = with_condition();
    let gate = vec![0usize];
    let mut child = gate.clone();
    child.push(0);
    assert_eq!(id_at(&app, &child), "C");
    // the condition itself: refused with a warning
    app.select(Some(child.clone()));
    app.move_selected(1);
    assert!(app.error.as_deref().unwrap().contains("cannot be shifted"));
    assert_eq!(id_at(&app, &child), "C");
    // the item right of it may not shift left past it
    app.error = None;
    let mut a = gate.clone();
    a.push(1);
    app.select(Some(a.clone()));
    app.move_selected(-1);
    assert_eq!(id_at(&app, &child), "C");
    assert_eq!(id_at(&app, &a), "A");
    assert!(app.error.is_none());
    // ordinary shifting still works between events
    app.move_selected(1);
    assert_eq!(id_at(&app, &a), "B");
}

#[test]
fn selected_opens_the_event_editor_for_a_primary_event() {
    let mut app = theatre();
    app.select(Some(vec![0, 1])); // MAINS_FAIL
    app.edit_selected();
    assert!(app.events_open);
    assert_eq!(app.event_form.id, "MAINS_FAIL");
    // anything else goes to the edit panel
    let mut app = theatre();
    app.select(Some(vec![0]));
    app.edit_selected();
    assert!(!app.events_open && app.focus_details);
    // nothing selected: a message
    let mut app = theatre();
    app.edit_selected();
    assert!(app.error.is_some());
}

#[test]
fn edit_menu_has_shift_and_selected_and_double_click_edits() {
    use eframe::egui::{Event, Modifiers, PointerButton, Pos2};
    let mut h = harness_for(theatre());
    h.run();
    h.state_mut().select(Some(vec![0, 1]));
    h.run();
    h.get_by_label("Edit").click();
    h.run();
    h.get_by_label("Shift Left");
    h.get_by_label("Shift Right");
    h.get_by_label("Selected...").click();
    h.run();
    assert!(
        h.state().events_open,
        "Edit > Selected... opens the event editor"
    );

    // a double-click on a symbol selects it and edits it
    let mut app = theatre();
    app.select(None);
    let mut h = Harness::builder()
        .with_size(vec2(1280.0, 800.0))
        .with_step_dt(0.05)
        .build_eframe(move |_| app);
    h.run();
    // select something else first: the edit panel appears and the tree refits,
    // so the symbol does not move between the two clicks
    h.state_mut().select(Some(vec![0]));
    h.run();
    h.run();
    h.state_mut().auto_fit = false;
    let scene = h.state().scene().unwrap();
    let b = scene.items.iter().find(|i| i.path == [0, 1]).unwrap().bbox;
    let (z, origin) = (h.state().zoom, h.state().canvas.min + h.state().pan);
    let pos = Pos2::new(
        origin.x + (b[0] + b[2]) / 2.0 * z,
        origin.y + (b[1] + b[3]) / 2.0 * z,
    );
    h.event(Event::PointerMoved(pos));
    for _ in 0..2 {
        for pressed in [true, false] {
            h.event(Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            });
        }
        // one frame per click: egui needs the clicks within 0.3 s
        h.step();
    }
    h.run();
    assert_eq!(h.state().selected.as_deref(), Some(&[0usize, 1][..]));
    assert!(h.state().events_open, "double-click edits the symbol");
}
