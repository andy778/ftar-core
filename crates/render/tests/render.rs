use ftar_core::project::Project;
use ftar_core::{EventType, PrimaryEventDb, Tree};
use ftar_render::svg::{wrap, xml_escape};
use ftar_render::{print_html, svg_to_png, tree_svg, PrintOptions, SvgOptions};
use resvg::tiny_skia::Pixmap;

const DATA: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../core/tests/data");

fn load(name: &str) -> Project {
    Project::load(format!("{DATA}/{name}")).unwrap()
}

fn decode(png: &[u8]) -> Pixmap {
    Pixmap::decode_png(png).expect("valid PNG")
}

#[test]
fn svg_names_every_event_and_gate() {
    let p = load("theatre.fta");
    let svg = tree_svg(&p.tree, &p.db, &SvgOptions::default());
    assert!(svg.starts_with("<svg "));
    for id in ["RELAY_FAIL", "GEN_FAIL", "MAINS_FAIL", "Backup_Fail", "Top"] {
        assert!(svg.contains(id), "{id}");
    }
    // gates show their ids, as in OpenFTA
    assert!(svg.contains(">AND1<") && svg.contains(">Or1<"), "{svg}");
    // probabilities as OpenFTA prints them (%g), descriptions from the database
    assert!(svg.contains(">0.03<") && svg.contains(">0.02<"), "{svg}");
    assert!(svg.contains(">Mains Power<"), "{svg}");
    let off = tree_svg(
        &p.tree,
        &p.db,
        &SvgOptions {
            probabilities: false,
            ..Default::default()
        },
    );
    assert!(!off.contains(">0.03<"));
}

#[test]
fn text_is_escaped_so_the_svg_stays_valid() {
    let mut p = load("theatre.fta");
    p.tree.root.text = "Fish & <chips> \"quoted\" 'x'".into();
    p.tree.root.children[0].children[1].id = "A&B<C>".into();
    let svg = tree_svg(
        &p.tree,
        &p.db,
        &SvgOptions {
            title: "T & <T>".into(),
            ..Default::default()
        },
    );
    assert!(svg.contains("A&amp;B&lt;C&gt;") && svg.contains("T &amp; &lt;T&gt;"));
    assert!(!svg.contains("A&B<C>"));
    svg_to_png(&svg, 1.0).expect("escaped SVG parses");
    assert_eq!(
        xml_escape("a\u{1}b"),
        "ab",
        "control characters are dropped"
    );
}

#[test]
fn wrapping_respects_width_and_line_limit() {
    assert_eq!(wrap("one two three", 7, 5), ["one two", "three"]);
    let w = wrap(
        "a very long description of a failing component in the plant",
        12,
        2,
    );
    assert_eq!(w.len(), 2);
    assert!(w.iter().all(|l| l.chars().count() <= 12));
    assert!(w[1].ends_with("..."));
    assert_eq!(wrap("", 10, 3), Vec::<String>::new());
    assert!(
        wrap("Supercalifragilisticexpialidocious", 10, 2)[0]
            .chars()
            .count()
            <= 10
    );
}

#[test]
fn png_has_the_expected_size_and_real_content() {
    let p = load("lift.fta");
    let svg = tree_svg(&p.tree, &p.db, &SvgOptions::default());
    let one = decode(&svg_to_png(&svg, 1.0).unwrap());
    let two = decode(&svg_to_png(&svg, 2.0).unwrap());
    assert_eq!(
        (two.width(), two.height()),
        (one.width() * 2, one.height() * 2)
    );
    assert!(one.width() > 1500, "lift is a wide tree: {}", one.width());
    let px = one.pixels();
    let non_white = px
        .iter()
        .filter(|c| !(c.red() > 250 && c.green() > 250 && c.blue() > 250))
        .count();
    assert!(
        non_white > 3000,
        "image looks empty: {non_white} drawn pixels"
    );
    // white background, opaque corners
    let corner = px[0];
    assert!(corner.red() == 255 && corner.alpha() == 255);
}

#[test]
fn transparent_background_option() {
    let p = load("theatre.fta");
    let svg = tree_svg(
        &p.tree,
        &p.db,
        &SvgOptions {
            background: false,
            ..Default::default()
        },
    );
    let img = decode(&svg_to_png(&svg, 1.0).unwrap());
    assert_eq!(img.pixels()[0].alpha(), 0);
}

#[test]
fn every_example_tree_renders() {
    for f in [
        "theatre.fta",
        "lift.fta",
        "top.fta",
        "threemotor/Three Motor Example (Motor 2 Only).fta",
    ] {
        let p = load(f);
        let svg = tree_svg(&p.tree, &p.db, &SvgOptions::default());
        let png = svg_to_png(&svg, 1.0).unwrap_or_else(|e| panic!("{f}: {e}"));
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n", "{f}");
    }
}

#[test]
fn bad_sizes_and_bad_svg_are_errors_not_panics() {
    let svg = tree_svg(
        &Tree::default(),
        &PrimaryEventDb::default(),
        &SvgOptions::default(),
    );
    for bad in [0.0, -1.0, f32::NAN, f32::INFINITY] {
        assert!(svg_to_png(&svg, bad).is_err(), "{bad}");
    }
    assert!(svg_to_png(&svg, 100_000.0).is_err());
    assert!(svg_to_png("not svg at all", 1.0).is_err());
    assert!(svg_to_png("", 1.0).is_err());
}

#[test]
fn every_node_kind_can_be_drawn() {
    use EventType::*;
    let mut tree = Tree::default();
    for (i, k) in [
        Basic,
        Undeveloped,
        External,
        Dormant,
        ConditionAnalysed,
        ConditionNotAnalysed,
        TransferIn,
        TransferOut,
        Intermediate,
        And,
        Xor,
        PriorityAnd,
        Inhibit,
    ]
    .into_iter()
    .enumerate()
    {
        let mut n = ftar_core::edit::new_node(k, &format!("N{i}"));
        if k == Intermediate {
            n.text = "An intermediate event with a rather long description that must wrap".into();
        }
        tree.root.children[0].children.push(n);
    }
    let svg = tree_svg(&tree, &PrimaryEventDb::default(), &SvgOptions::default());
    for shape in ["<polygon", "<polyline", "stroke-dasharray"] {
        assert!(svg.contains(shape), "{shape}");
    }
    svg_to_png(&svg, 0.5).unwrap();
}

#[test]
fn print_page_contains_drawing_title_note_and_report() {
    let p = load("theatre.fta");
    let html = print_html(
        &p.tree,
        &p.db,
        &PrintOptions {
            title: "Plant <A> & B".into(),
            probabilities: true,
            report: Some("Cut sets: A < B & C".into()),
            auto_print: true,
            ..Default::default()
        },
    );
    assert!(html.contains("<svg ") && html.contains("window.print()"));
    assert!(html.contains("<title>Plant &lt;A&gt; &amp; B</title>"));
    assert!(
        html.contains("Failure Of An Electrical Supply"),
        "note shown"
    );
    assert!(html.contains("Cut sets: A &lt; B &amp; C"));
    assert!(html.contains("@page"));
    let quiet = print_html(&p.tree, &p.db, &PrintOptions::default());
    assert!(!quiet.contains("window.print()") && !quiet.contains("class=\"report\""));
}

#[test]
fn header_names_the_database_the_events_come_from() {
    let p = ftar_core::project::Project::load(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../core/tests/data/lift.fta"
    ))
    .unwrap();
    let titled = SvgOptions {
        title: "lift.fta".into(),
        ..SvgOptions::default()
    };
    let svg = tree_svg(&p.tree, &p.db, &titled);
    assert!(svg.contains("Database: lift.ped"));
    assert!(!tree_svg(&p.tree, &p.db, &SvgOptions::default()).contains("Database:"));
}

// ---------------------------------------------------------- page setup (#56)

mod page_setup {
    use super::*;
    use ftar_render::{page_count, print_pages, Fit, Orientation, PageSetup, PageSize, MAX_PAGES};

    fn sheet_size(svg: &str) -> (String, String) {
        let w = svg
            .split("width=\"")
            .nth(1)
            .unwrap()
            .split('"')
            .next()
            .unwrap();
        let h = svg
            .split("height=\"")
            .nth(1)
            .unwrap()
            .split('"')
            .next()
            .unwrap();
        (w.to_string(), h.to_string())
    }

    #[test]
    fn size_and_orientation_set_the_sheet() {
        let p = load("theatre.fta");
        let mut s = PageSetup::default();
        let pages = print_pages(&p.tree, &p.db, "t", true, &s);
        assert_eq!(pages.len(), 1);
        assert_eq!(sheet_size(&pages[0]), ("297.0mm".into(), "210.0mm".into()));
        s.orientation = Orientation::Portrait;
        s.size = PageSize::Letter;
        let pages = print_pages(&p.tree, &p.db, "t", true, &s);
        assert_eq!(sheet_size(&pages[0]), ("215.9mm".into(), "279.4mm".into()));
        assert!(pages[0].contains("RELAY_FAIL"));
        let html = print_html(
            &p.tree,
            &p.db,
            &PrintOptions {
                page: s,
                ..Default::default()
            },
        );
        assert!(html.contains("size: 215.9mm 279.4mm"));
    }

    #[test]
    fn split_spreads_the_drawing_over_sheets_and_fit_keeps_one() {
        let p = load("theatre.fta");
        let mut s = PageSetup {
            size: PageSize::A5,
            scale_percent: 200.0,
            ..Default::default()
        };
        assert_eq!(
            page_count(&p.tree, &p.db, true, &s),
            1,
            "fit mode is one sheet"
        );
        s.fit = Fit::SplitPages;
        let n = page_count(&p.tree, &p.db, true, &s);
        assert!(n > 1);
        let pages = print_pages(&p.tree, &p.db, "t", true, &s);
        assert_eq!(pages.len(), n);
        assert!(pages[1].contains(&format!("Page 2 of {n}")));
        // every sheet rasterises
        for pg in &pages {
            svg_to_png(pg, 0.3).unwrap();
        }
        // smaller scale needs no more sheets
        s.scale_percent = 50.0;
        assert!(page_count(&p.tree, &p.db, true, &s) <= n);
    }

    #[test]
    fn hostile_settings_never_panic_and_cap_the_page_count() {
        let p = load("theatre.fta");
        let s = PageSetup {
            size: PageSize::A5,
            fit: Fit::SplitPages,
            margin_mm: f64::NAN,
            scale_percent: f64::INFINITY,
            ..Default::default()
        };
        assert!(print_pages(&p.tree, &p.db, "t", true, &s).len() <= MAX_PAGES);
        let s = PageSetup {
            margin_mm: 1e9,
            scale_percent: -5.0,
            ..s
        };
        assert!(print_pages(&p.tree, &p.db, "t", true, &s).len() <= MAX_PAGES);
        assert!(PageSetup::from_options(Some("A9"), None, None, None, None).is_err());
        assert!(PageSetup::from_options(None, Some("sideways"), None, None, None).is_err());
        assert!(PageSetup::from_options(None, None, Some("zoom"), None, None).is_err());
        let ok = PageSetup::from_options(
            Some("a3"),
            Some("Portrait"),
            Some("split"),
            Some(50.0),
            Some(5.0),
        )
        .unwrap();
        assert_eq!(
            (ok.size, ok.orientation, ok.fit),
            (PageSize::A3, Orientation::Portrait, Fit::SplitPages)
        );
    }
}

#[test]
fn report_page_holds_only_the_escaped_report_and_prints_on_load() {
    use ftar_render::page::PageSetup;
    use ftar_render::report_html;
    let html = report_html(
        "T <1> - Probability report",
        "p < 1 & q",
        true,
        &PageSetup::default(),
    );
    assert!(html.contains("<title>T &lt;1&gt; - Probability report</title>"));
    assert!(html.contains("<pre class=\"report\">p &lt; 1 &amp; q</pre>"));
    assert!(html.contains("window.print()") && html.contains("@page"));
    assert!(!html.contains("<svg"), "no drawing on a report page");
    let quiet = report_html("T", "x", false, &PageSetup::default());
    assert!(!quiet.contains("window.print()"));
}

#[test]
fn theme_is_light_by_default_and_round_trips() {
    use ftar_render::{Preferences, Theme};
    assert_eq!(Preferences::default().theme, Theme::Light);
    for t in Theme::ALL {
        let p = Preferences {
            theme: t,
            ..Preferences::default()
        };
        assert_eq!(Preferences::from_text(&p.to_text()).theme, t);
    }
    assert_eq!(Preferences::from_text("theme = bogus").theme, Theme::Light);
}

#[test]
fn jpeg_is_drawn_on_white_and_clamps_quality() {
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="30"><rect width="20" height="30" fill="#000000"/></svg>"##;
    for q in [0u8, 100, 255] {
        let b = ftar_render::svg_to_jpeg(svg, 1.0, q).unwrap();
        assert_eq!(&b[..2], &[0xff, 0xd8]);
    }
    assert!(ftar_render::svg_to_jpeg("not svg", 1.0, 90).is_err());
    assert!(ftar_render::svg_to_jpeg(svg, 0.0, 90).is_err());
}
