//! A printable HTML page: the drawing on sheets of the chosen page setup, then the
//! note and an optional text report. Opening it in a browser and printing gives
//! paper or a PDF.

use ftar_core::{PrimaryEventDb, Tree};

use crate::page::{print_pages, PageSetup};
use crate::svg::xml_escape;

#[derive(Debug, Clone, Default)]
pub struct PrintOptions {
    pub title: String,
    pub probabilities: bool,
    /// Plain-text report appended on its own page(s), if any.
    pub report: Option<String>,
    /// Open the print dialog as soon as the page loads.
    pub auto_print: bool,
    /// Sheet size, orientation and fit/split.
    pub page: PageSetup,
}

pub fn print_html(tree: &Tree, db: &PrimaryEventDb, opts: &PrintOptions) -> String {
    let pages = print_pages(tree, db, &opts.title, opts.probabilities, &opts.page);
    let title = xml_escape(&opts.title);
    let (sw, sh) = opts.page.sheet_mm();
    let m = opts.page.content_margin();
    let mut h = String::new();
    h.push_str("<!doctype html>\n<html lang=\"en\"><head><meta charset=\"utf-8\">\n");
    h.push_str(&format!("<title>{title}</title>\n"));
    h.push_str(&format!(
        "<style>\n\
         @page {{ size: {sw:.1}mm {sh:.1}mm; margin: 0; }}\n\
         body {{ font-family: 'DejaVu Sans', Arial, sans-serif; margin: 0; color: #111; }}\n\
         section.page {{ width: {sw:.1}mm; height: {sh:.1}mm; overflow: hidden; page-break-after: always; break-after: page; }}\n\
         section.page svg {{ display: block; }}\n\
         section.text {{ padding: {m:.1}mm; }}\n\
         .note {{ white-space: pre-wrap; font-size: 10pt; margin: 0 0 10pt 0; color: #333; }}\n\
         pre.report {{ font-size: 8.5pt; line-height: 1.25; }}\n\
         @media screen {{ section.page {{ border-bottom: 1px solid #ccc; }} }}\n\
         </style>\n"
    ));
    if opts.auto_print {
        h.push_str(
            "<script>window.addEventListener('load', function () { window.print(); });</script>\n",
        );
    }
    h.push_str("</head><body>\n");
    for p in &pages {
        h.push_str("<section class=\"page\">\n");
        h.push_str(p);
        h.push_str("</section>\n");
    }
    let note = tree.note.trim();
    if !note.is_empty() || opts.report.is_some() {
        h.push_str("<section class=\"text\">\n");
        if !note.is_empty() {
            h.push_str(&format!("<p class=\"note\">{}</p>\n", xml_escape(note)));
        }
        if let Some(r) = &opts.report {
            h.push_str(&format!("<pre class=\"report\">{}</pre>\n", xml_escape(r)));
        }
        h.push_str("</section>\n");
    }
    h.push_str("</body></html>\n");
    h
}
