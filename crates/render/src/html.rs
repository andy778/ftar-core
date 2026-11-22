//! A printable HTML page: title, note, the drawing and optionally a text report.
//! Opening it in a browser and printing gives paper or a PDF.

use ftar_core::{PrimaryEventDb, Tree};

use crate::prefs::Preferences;
use crate::svg::{tree_svg_with, xml_escape, SvgOptions};

#[derive(Debug, Clone, Default)]
pub struct PrintOptions {
    pub title: String,
    pub probabilities: bool,
    /// Plain-text report appended on its own page(s), if any.
    pub report: Option<String>,
    /// Open the print dialog as soon as the page loads.
    pub auto_print: bool,
}

pub fn print_html(tree: &Tree, db: &PrimaryEventDb, opts: &PrintOptions) -> String {
    print_html_with(tree, db, opts, &Preferences::default())
}

/// Like [`print_html`], with the user's colours and monochrome mode.
pub fn print_html_with(
    tree: &Tree,
    db: &PrimaryEventDb,
    opts: &PrintOptions,
    prefs: &Preferences,
) -> String {
    let svg = tree_svg_with(
        tree,
        db,
        &SvgOptions {
            title: String::new(),
            probabilities: opts.probabilities,
            background: false,
        },
        prefs,
    );
    let title = xml_escape(&opts.title);
    let mut h = String::new();
    h.push_str("<!doctype html>\n<html lang=\"en\"><head><meta charset=\"utf-8\">\n");
    h.push_str(&format!("<title>{title}</title>\n"));
    h.push_str(
        "<style>\n\
         @page { size: A4 landscape; margin: 12mm; }\n\
         body { font-family: 'DejaVu Sans', Arial, sans-serif; margin: 0; color: #111; }\n\
         h1 { font-size: 18pt; margin: 0 0 4pt 0; }\n\
         .note { white-space: pre-wrap; font-size: 10pt; margin: 0 0 10pt 0; color: #333; }\n\
         .tree svg { width: 100%; height: auto; max-height: 170mm; }\n\
         pre.report { page-break-before: always; font-size: 8.5pt; line-height: 1.25; }\n\
         </style>\n",
    );
    if opts.auto_print {
        h.push_str(
            "<script>window.addEventListener('load', function () { window.print(); });</script>\n",
        );
    }
    h.push_str("</head><body>\n");
    h.push_str(&format!("<h1>{title}</h1>\n"));
    if !tree.note.trim().is_empty() {
        h.push_str(&format!(
            "<p class=\"note\">{}</p>\n",
            xml_escape(tree.note.trim())
        ));
    }
    h.push_str("<div class=\"tree\">\n");
    h.push_str(&svg);
    h.push_str("</div>\n");
    if let Some(r) = &opts.report {
        h.push_str(&format!("<pre class=\"report\">{}</pre>\n", xml_escape(r)));
    }
    h.push_str("</body></html>\n");
    h
}
