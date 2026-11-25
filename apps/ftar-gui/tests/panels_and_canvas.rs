//! Behaviour of the menus, the analysis panel and the canvas that the other headless tests
//! leave out: root-only guards, the database field, each tab's parameters and results, the
//! clipboard events real backends send, scroll-wheel zoom and the canvas scrollbars.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use eframe::egui::{self, pos2, vec2, Key, Modifiers, PointerButton};
use egui_kittest::{
    kittest::{NodeT, Queryable},
    Harness,
};
use ftar_gui::{App, Tab};

/// A fixture copied into its own temp directory, so analyses that write report files beside
/// the tree never touch the repository.
fn data(name: &str) -> PathBuf {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static N: AtomicUsize = AtomicUsize::new(0);
    let src = PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../crates/core/tests/data"
    ));
    let dir = std::env::temp_dir().join(format!(
        "ftar_gui_panels_{}_{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    // copy the fixture's folder (and so its database and transfer trees) whole
    let (sub, file) = name.rsplit_once('/').unwrap_or(("", name));
    for e in std::fs::read_dir(src.join(sub)).unwrap().flatten() {
        if e.path().is_file() {
            std::fs::copy(e.path(), dir.join(e.file_name())).unwrap();
        }
    }
    dir.join(file)
}

fn harness(file: &str) -> Harness<'static, App> {
    let mut app = App::default();
    app.open(data(file));
    let mut h = Harness::builder()
        .with_size(vec2(1280.0, 800.0))
        .build_eframe(move |_| app);
    h.run();
    h
}

fn node_count(app: &App) -> usize {
    app.doc.as_ref().unwrap().tree.nodes().len()
}

fn menu_item_disabled(h: &mut Harness<'static, App>, menu: &str, item: &str) -> bool {
    h.get_by_label(menu).click();
    h.run();
    let disabled = h
        .query_all_by_label_contains(item)
        .next()
        .unwrap_or_else(|| panic!("{item} in the {menu} menu"))
        .accesskit_node()
        .is_disabled();
    h.key_press(Key::Escape);
    h.run();
    disabled
}

// ------------------------------------------------------------------ the top event

#[test]
fn the_top_event_cannot_be_deleted_or_shifted() {
    let mut h = harness("theatre.fta");
    let before = node_count(h.state());
    h.state_mut().select(Some(vec![]));
    h.run();
    h.key_press(Key::Delete);
    h.run();
    assert_eq!(node_count(h.state()), before, "Delete leaves the top event");
    assert!(h.state().error.is_none(), "and does not even try");
    assert!(menu_item_disabled(&mut h, "Edit", "Shift Left"));
    assert!(menu_item_disabled(&mut h, "Edit", "Delete selected"));

    h.state_mut().select(Some(vec![0, 1]));
    h.run();
    assert!(!menu_item_disabled(&mut h, "Edit", "Shift Left"));
    assert!(!menu_item_disabled(&mut h, "Edit", "Delete selected"));
}

// ------------------------------------------------------------------ database field

#[test]
fn typing_a_database_name_applies_it_when_the_field_loses_focus() {
    let mut h = harness("theatre.fta");
    let field = h.get_by_label("database");
    field.click();
    h.run();
    h.get_by_label("database").type_text("other");
    h.run();
    assert_eq!(
        h.state().doc.as_ref().unwrap().tree.db_name.as_deref(),
        Some("theatre.ped"),
        "nothing changes while typing"
    );
    h.key_press(Key::Tab);
    h.run();
    let name = h.state().doc.as_ref().unwrap().tree.db_name.clone();
    // the text lands at the cursor, wherever the click put it
    assert!(
        name.as_deref()
            .is_some_and(|n| n != "theatre.ped" && n.contains("other")),
        "{name:?}"
    );
    assert!(h.state().dirty, "the change is an edit");
}

// ------------------------------------------------------------------ analysis panel

#[test]
fn each_tab_shows_its_own_parameters() {
    let mut h = harness("theatre.fta");
    for (tab, shown, hidden) in [
        (Tab::CutSets, &[][..], &["terms", "trials", "seed"][..]),
        (
            Tab::Probability,
            &["terms", "unit time"][..],
            &["trials", "seed"][..],
        ),
        (
            Tab::MonteCarlo,
            &["trials", "unit time", "seed", "method"][..],
            &["terms"][..],
        ),
    ] {
        h.state_mut().tab = tab;
        h.run();
        for l in shown {
            assert!(h.query_by_label(l).is_some(), "{tab:?} shows {l}");
        }
        for l in hidden {
            assert!(h.query_by_label(l).is_none(), "{tab:?} hides {l}");
        }
    }
}

#[test]
fn probability_bounds_alternate_upper_and_lower() {
    let mut h = harness("theatre.fta");
    h.state_mut().tab = Tab::Probability;
    h.state_mut().run_analysis();
    h.run();
    // the result lines carry their text as the node's value
    let line = |h: &Harness<'static, App>, k: usize| {
        let want = format!("{k:>2} terms");
        let values: Vec<String> = h
            .query_all_by_label_contains(&want)
            .filter_map(|n| n.accesskit_node().value().map(|v| v.to_string()))
            .collect();
        values
            .into_iter()
            .find(|v| v.starts_with(&want))
            .unwrap_or_else(|| panic!("line for {k} terms"))
    };
    assert!(line(&h, 1).ends_with("(upper)"), "{}", line(&h, 1));
    assert!(line(&h, 2).ends_with("(lower)"), "{}", line(&h, 2));
}

#[test]
fn more_than_forty_failure_modes_are_summarised() {
    let mut h = harness("threemotor/T.fta");
    h.state_mut().tab = Tab::MonteCarlo;
    h.state_mut().trials = 200_000;
    h.state_mut().run_analysis();
    h.run();
    let modes = h.state().mc.as_ref().unwrap().modes.len();
    assert!(modes > 40, "the three-motor tree has {modes} failure modes");
    h.get_by_label(&format!("... {} more (export to see all)", modes - 40));
}

// ------------------------------------------------------------------ clipboard events

/// Backends deliver cut, copy and paste as clipboard events rather than key presses.
#[test]
fn clipboard_events_copy_cut_and_paste_branches() {
    let mut h = harness("theatre.fta");
    h.state_mut().select(Some(vec![0, 1]));
    h.run();
    h.event(egui::Event::Copy);
    h.run();
    assert!(h.state().clipboard.is_some(), "Copy event copies");

    let before = node_count(h.state());
    h.state_mut().select(Some(vec![0, 0, 0]));
    h.run();
    h.event(egui::Event::Paste(String::new()));
    h.run();
    assert!(node_count(h.state()) > before, "Paste event pastes");

    let pasted = node_count(h.state());
    h.event(egui::Event::Cut);
    h.run();
    assert!(node_count(h.state()) < pasted, "Cut event cuts");
}

// ------------------------------------------------------------------ canvas

/// The point of the drawing under the pointer, in drawing units.
fn under(app: &App, m: egui::Pos2) -> egui::Vec2 {
    (m - (app.canvas.min + app.pan)) / app.zoom
}

#[test]
fn the_scroll_wheel_zooms_about_the_pointer() {
    let mut h = harness("theatre.fta");
    let c = h.state().canvas;
    let m = pos2(c.min.x + c.width() * 0.3, c.min.y + c.height() * 0.4);
    h.event(egui::Event::PointerMoved(m));
    h.run();
    let (z0, w0) = (h.state().zoom, under(h.state(), m));
    h.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: vec2(0.0, 120.0),
        phase: egui::TouchPhase::Move,
        source: egui::MouseWheelSource::Wheel,
        modifiers: Modifiers::NONE,
    });
    // egui spreads the wheel over several frames: the point must stay put after each one
    for frame in 0..30 {
        h.step();
        let w = under(h.state(), m);
        assert!(
            (w - w0).length() < 0.5,
            "frame {frame}: the point under the pointer stays put: {w0:?} -> {w:?}"
        );
    }
    let a = h.state();
    // 120 points at 0.2 % each is about 27 % (e^0.24), not a jump to the zoom limit
    let factor = a.zoom / z0;
    assert!(
        (1.15..1.4).contains(&factor),
        "scrolling up zooms in by about a quarter: {z0} -> {}",
        a.zoom
    );
    assert!(!a.auto_fit, "a manual zoom stops auto-fit");
}

#[test]
fn dragging_the_canvas_pans_the_drawing_with_the_pointer() {
    let mut h = harness("theatre.fta");
    let c = h.state().canvas;
    let start = c.center();
    let pan0 = h.state().pan;
    h.event(egui::Event::PointerMoved(start));
    h.event(egui::Event::PointerButton {
        pos: start,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    h.run();
    for i in 1..=10 {
        h.event(egui::Event::PointerMoved(start + vec2(6.0, 4.0) * i as f32));
        h.run();
    }
    let end = start + vec2(60.0, 40.0);
    h.event(egui::Event::PointerButton {
        pos: end,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    h.run();
    let a = h.state();
    assert!(
        (a.pan - pan0 - vec2(60.0, 40.0)).length() < 1.0,
        "the drawing follows the pointer: {pan0:?} -> {:?}",
        a.pan
    );
    assert!(!a.auto_fit, "a drag stops auto-fit");
}

#[test]
fn a_manual_view_survives_resizing_the_window() {
    let mut h = harness("theatre.fta");
    h.state_mut().set_zoom(2.0);
    h.run();
    let (zoom, pan) = (h.state().zoom, h.state().pan);
    assert!(!h.state().auto_fit);
    h.set_size(vec2(1000.0, 700.0));
    h.run();
    h.run();
    let a = h.state();
    assert_eq!(a.zoom, zoom, "no refit after a resize");
    assert_eq!(a.pan, pan);
}

#[test]
fn clicking_the_vertical_scrollbar_track_scrolls_the_drawing() {
    let mut h = harness("theatre.fta");
    h.state_mut().set_zoom(3.0);
    h.run();
    let a = h.state();
    let c = a.canvas;
    assert!(
        a.scroll_extent.y > c.height(),
        "zoomed in, the drawing overflows"
    );
    let pan0 = a.pan;
    let on_track = pos2(c.max.x - 3.0, c.max.y - 30.0);
    h.event(egui::Event::PointerMoved(on_track));
    h.event(egui::Event::PointerButton {
        pos: on_track,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    h.run();
    h.event(egui::Event::PointerButton {
        pos: on_track,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    h.run();
    let a = h.state();
    assert!(
        a.pan.y < pan0.y - 10.0,
        "the drawing moves up: {pan0:?} -> {:?}",
        a.pan
    );
    assert!(
        (a.pan.x - pan0.x).abs() < 0.5,
        "and not sideways: {pan0:?} -> {:?}",
        a.pan
    );
}

#[test]
fn the_theme_menu_marks_the_current_theme() {
    let mut h = harness("theatre.fta");
    let mut p = h.state().prefs.clone();
    p.theme = ftar_render::Theme::Dark;
    h.state_mut().prefs = p;
    h.run();
    h.get_by_label("View").click();
    h.run();
    h.get_by_label("Theme").hover();
    h.run();
    let selected = |h: &Harness<'static, App>, l: &str| {
        h.get_by_label(l).accesskit_node().toggled() == Some(eframe::egui::accesskit::Toggled::True)
    };
    assert!(selected(&h, "Dark"));
    assert!(!selected(&h, "Light"));
    assert!(!selected(&h, "System"));
}

#[test]
fn clicking_the_horizontal_scrollbar_track_scrolls_the_drawing() {
    let mut h = harness("theatre.fta");
    h.state_mut().set_zoom(3.0);
    h.run();
    let a = h.state();
    let c = a.canvas;
    assert!(
        a.scroll_extent.x > c.width(),
        "zoomed in, the drawing overflows"
    );
    let pan0 = a.pan;
    let on_track = pos2(c.max.x - 40.0, c.max.y - 3.0);
    for pressed in [true, false] {
        h.event(egui::Event::PointerMoved(on_track));
        h.event(egui::Event::PointerButton {
            pos: on_track,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
        h.run();
    }
    let a = h.state();
    assert!(
        a.pan.x < pan0.x - 10.0,
        "the drawing moves left: {pan0:?} -> {:?}",
        a.pan
    );
    assert!(
        (a.pan.y - pan0.y).abs() < 0.5,
        "and not up or down: {pan0:?} -> {:?}",
        a.pan
    );
    assert!(!a.auto_fit, "scrolling stops auto-fit");
}

#[test]
fn exactly_forty_failure_modes_are_all_listed() {
    let mut h = harness("threemotor/T.fta");
    h.state_mut().tab = Tab::MonteCarlo;
    h.state_mut().trials = 200_000;
    h.state_mut().run_analysis();
    h.run();
    let shown = |h: &Harness<'static, App>| {
        h.query_all_by_label_contains("more (export to see all)")
            .count()
    };
    h.state_mut().mc.as_mut().unwrap().modes.truncate(40);
    h.run();
    assert_eq!(shown(&h), 0, "40 modes fit, no summary line");
    h.state_mut().tab = Tab::CutSets;
    h.run();
    h.state_mut().tab = Tab::MonteCarlo;
    h.state_mut().run_analysis();
    h.state_mut().mc.as_mut().unwrap().modes.truncate(41);
    h.run();
    h.get_by_label("... 1 more (export to see all)");
}

// ------------------------------------------------------------------ tabs and selection

#[test]
fn a_tab_click_picks_its_report_and_a_chosen_report_stays() {
    use ftar_core::report::ReportKind;
    let mut h = harness("theatre.fta");
    h.get_by_label("Probability").click();
    h.run();
    assert_eq!(h.state().tab, Tab::Probability);
    assert_eq!(
        h.state().report_kind,
        ReportKind::Probability,
        "the report follows the tab"
    );

    // a report picked by hand is kept while the tab stays the same
    h.state_mut().report_kind = ReportKind::MonteCarlo;
    h.run();
    h.run();
    assert_eq!(h.state().report_kind, ReportKind::MonteCarlo);
}

#[test]
fn clicking_a_symbol_selects_it_when_zoomed_in() {
    let mut h = harness("theatre.fta");
    h.state_mut().set_zoom(2.5);
    h.run();
    let scene = h.state().scene().expect("scene");
    let b = scene.items.iter().find(|i| i.path == [0, 1]).unwrap().bbox;
    let (z, origin) = (h.state().zoom, h.state().canvas.min + h.state().pan);
    assert!((z - 2.5).abs() < 1e-4, "zoomed in: {z}");
    let pos = pos2(
        origin.x + (b[0] + b[2]) / 2.0 * z,
        origin.y + (b[1] + b[3]) / 2.0 * z,
    );
    assert!(
        h.state().canvas.contains(pos),
        "the symbol is on screen at {pos:?}"
    );
    let version = h.state().version;
    h.event(egui::Event::PointerMoved(pos));
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
        h.run();
    }
    assert_eq!(h.state().selected.as_deref(), Some(&[0usize, 1][..]));
    assert!(h.state().version > version, "a new selection redraws");
}
