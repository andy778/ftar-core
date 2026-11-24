//! Report > View reopens the report files the last analyses wrote (#55).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use eframe::egui::vec2;
use egui_kittest::{
    kittest::{NodeT, Queryable},
    Harness,
};
use ftar_core::report::ReportKind;
use ftar_gui::{App, Tab};

const DATA: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../crates/core/tests/data");

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("ftar_gui_rv_{}_{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn harness_for(app: App) -> Harness<'static, App> {
    Harness::builder()
        .with_size(vec2(1280.0, 800.0))
        .build_eframe(move |_| app)
}

fn app_in(dir: &std::path::Path) -> App {
    let tree = dir.join("theatre.fta");
    std::fs::copy(PathBuf::from(DATA).join("theatre.fta"), &tree).unwrap();
    std::fs::copy(
        PathBuf::from(DATA).join("theatre.ped"),
        dir.join("theatre.ped"),
    )
    .unwrap();
    let mut app = App::default();
    app.open(tree);
    app
}

#[test]
fn report_view_reopens_the_last_saved_reports() {
    let dir = scratch("view");
    let mut app = app_in(&dir);
    // Nothing analysed yet: no report file, nothing to view.
    for k in [
        ReportKind::CutSets,
        ReportKind::Probability,
        ReportKind::MonteCarlo,
    ] {
        assert!(!app.saved_report_exists(k));
        assert!(app.view_saved_report(k).is_err());
    }
    app.run_analysis();
    assert!(dir.join("theatre.crp").is_file());
    assert!(app.saved_report_exists(ReportKind::CutSets));
    assert!(!app.saved_report_exists(ReportKind::Probability));
    app.tab = Tab::Probability;
    app.run_analysis();
    assert!(dir.join("theatre.prp").is_file());

    let mut h = harness_for(app);
    h.run();
    h.get_by_label("Report").click();
    h.run();
    h.get_all_by_label("View").last().unwrap().click(); // the submenu, after the top-level View menu
    h.run();
    h.get_by_label("Probability Report").click();
    h.run();
    let shown = h.state().report_view.clone().unwrap().1;
    assert_eq!(
        shown,
        std::fs::read_to_string(dir.join("theatre.prp")).unwrap()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn report_view_is_disabled_without_the_file() {
    let dir = scratch("off");
    let app = app_in(&dir);
    let mut h = harness_for(app);
    h.run();
    h.get_by_label("Report").click();
    h.run();
    h.get_all_by_label("View").last().unwrap().click(); // the submenu, after the top-level View menu
    h.run();
    h.get_by_label("Monte Carlo Report").click();
    h.run();
    assert!(h.state().report_view.is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn report_print_writes_the_saved_report_as_its_own_page() {
    use ftar_core::report::ReportKind;
    let dir = scratch("print");
    let mut app = app_in(&dir);
    // nothing saved yet: an error, not a page
    assert!(!app.saved_report_exists(ReportKind::Probability));
    assert!(app.prepare_report_print(ReportKind::Probability).is_err());
    app.tab = Tab::Probability;
    app.run_analysis();
    let saved = std::fs::read_to_string(dir.join("theatre.prp")).unwrap();
    let page = app.prepare_report_print(ReportKind::Probability).unwrap();
    let html = std::fs::read_to_string(&page).unwrap();
    assert!(html.contains("window.print()"));
    assert!(html.contains("Probability report"));
    assert!(html.contains("Probabilities Analysis"), "{html}");
    assert!(!html.contains("<svg"));
    // the page shows the saved file, not a fresh analysis
    std::fs::write(dir.join("theatre.prp"), "edited <on> disk").unwrap();
    let page = app.prepare_report_print(ReportKind::Probability).unwrap();
    assert!(std::fs::read_to_string(page)
        .unwrap()
        .contains("edited &lt;on&gt; disk"));
    let _ = saved;
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn report_print_menu_follows_the_saved_files() {
    let dir = scratch("printmenu");
    let mut app = app_in(&dir);
    app.tab = Tab::CutSets;
    app.run_analysis();
    let mut h = harness_for(app);
    h.run();
    h.get_by_label("Report").click();
    h.run();
    h.get_by_label("Print").click();
    h.run();
    let enabled = |h: &Harness<'_, App>, label: &str| {
        !h.get_all_by_label(label)
            .last()
            .unwrap()
            .accesskit_node()
            .is_disabled()
    };
    assert!(
        enabled(&h, "Qualitative Report"),
        "the cut sets report exists"
    );
    assert!(!enabled(&h, "Probability Report"));
    assert!(!enabled(&h, "Monte Carlo Report"));
    let _ = std::fs::remove_dir_all(&dir);
}
