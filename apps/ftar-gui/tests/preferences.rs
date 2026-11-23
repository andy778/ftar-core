//! Drawing preferences: colours and monochrome reach exports and print, are saved between
//! sessions, and the window opens (headless, no GPU).

use std::path::PathBuf;

use eframe::egui::vec2;
use egui_kittest::{kittest::Queryable, Harness};
use ftar_core::EventType;
use ftar_gui::App;
use ftar_render::{Preferences, Symbol};

fn theatre() -> App {
    let mut app = App::default();
    app.open(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../crates/core/tests/data/theatre.fta"),
    );
    app
}

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("ftar_gui_prefs_{}_{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn colours_reach_svg_and_print() {
    let mut app = theatre();
    let mut p = Preferences::default();
    p.set_fill(Symbol::Basic, [1, 2, 3]);
    p.background = [9, 8, 7];
    app.set_prefs(p);
    let svg = app.svg_text().unwrap();
    assert!(svg.contains("#010203"), "basic fill");
    assert!(svg.contains("#090807"), "background");
    assert!(app.print_page(false).unwrap().contains("#010203"));
}

#[test]
fn monochrome_is_black_on_white() {
    let mut app = theatre();
    let colour = app.svg_text().unwrap();
    app.set_prefs(Preferences {
        monochrome: true,
        ..Preferences::default()
    });
    let mono = app.svg_text().unwrap();
    assert_ne!(colour, mono);
    assert!(!mono.contains("#d2fac8"));
    assert!(mono.contains("fill=\"#ffffff\""));
    assert!(mono.contains("stroke=\"#000000\""));
    let _ = EventType::Basic;
}

#[test]
fn saved_between_sessions() {
    let path = scratch("save").join("sub").join("preferences.txt");
    let mut a = App::default();
    a.use_prefs_file(path.clone());
    let mut p = Preferences {
        monochrome: true,
        ..Preferences::default()
    };
    p.set_fill(Symbol::And, [10, 20, 30]);
    a.set_prefs(p.clone());
    assert!(a.error.is_none());

    let mut b = App::default();
    b.use_prefs_file(path);
    assert_eq!(b.prefs, p);
}

#[test]
fn unreadable_file_gives_defaults() {
    let dir = scratch("bad");
    let path = dir.join("preferences.txt");
    std::fs::write(&path, [0xff, 0xfe, 0x00, b'=', b'#']).unwrap();
    let mut a = App::default();
    a.use_prefs_file(path);
    assert_eq!(a.prefs, Preferences::default());
}

#[test]
fn preferences_window_opens_and_switches_to_monochrome() {
    let mut app = theatre();
    app.prefs_open = true;
    let mut h = Harness::builder()
        .with_size(vec2(1280.0, 900.0))
        .build_eframe(move |_| app);
    h.run();
    h.get_by_label("Monochrome (for printing)").click();
    h.run();
    assert!(h.state().prefs.monochrome);
    assert!(h.get_all_by_label("Basic event").count() >= 1);
}

// ------------------------------------------------ #105 web browser launch command

#[test]
fn browser_command_is_split_without_a_shell_and_gets_the_page_last() {
    use ftar_gui::browser_command;
    use std::path::Path;
    let page = Path::new("/tmp/a b/page.html");
    assert_eq!(browser_command("", page), None);
    assert_eq!(browser_command("   ", page), None);
    let (prog, args) = browser_command("firefox", page).unwrap();
    assert_eq!(prog, "firefox");
    assert_eq!(args, vec!["/tmp/a b/page.html"]);
    let (prog, args) = browser_command("\"/opt/my browser/run\" --new-window  -p x", page).unwrap();
    assert_eq!(prog, "/opt/my browser/run");
    assert_eq!(args, vec!["--new-window", "-p", "x", "/tmp/a b/page.html"]);
    // shell characters are just text: nothing is interpreted
    let (prog, args) = browser_command("echo ; rm -rf ~", page).unwrap();
    assert_eq!(prog, "echo");
    assert_eq!(args[0], ";");
}

#[test]
fn browser_command_is_saved_with_the_preferences() {
    use ftar_render::Preferences;
    let p = Preferences {
        browser: "firefox --private-window".into(),
        ..Preferences::default()
    };
    assert_eq!(Preferences::from_text(&p.to_text()).browser, p.browser);
    assert_eq!(Preferences::default().browser, "");
    // a newline cannot split the setting into two lines
    let p = Preferences {
        browser: "a\nfocus = x".into(),
        ..Preferences::default()
    };
    assert_eq!(Preferences::from_text(&p.to_text()).browser, "a focus = x");
}

#[test]
fn preferences_window_has_the_browser_command_field() {
    let mut app = App::default();
    app.prefs_open = true;
    let mut h = Harness::builder()
        .with_size(vec2(1280.0, 800.0))
        .build_eframe(move |_| app);
    h.run();
    h.get_by_label("Web Browser Launch Command");
    h.get_by_label("Browse...");
    h.get_by_label("command").click();
    h.run();
    h.get_by_label("command").type_text("firefox");
    h.run();
    h.run();
    assert_eq!(h.state().prefs.browser, "firefox");
}
