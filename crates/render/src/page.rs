//! Page setup and print layout (OpenFTA's Page Setup / Print Preview).
//!
//! [`print_pages`] lays the tree drawing out on sheets of a chosen size and
//! orientation, either scaled to fit one sheet or split across several, and
//! returns one self-contained SVG per sheet. The same sheets back the HTML
//! print page and the GUI's print preview.

use ftar_core::{PrimaryEventDb, Tree};

use crate::prefs::Preferences;
use crate::svg::{tree_svg, tree_svg_with, xml_escape, SvgOptions};

/// Never produce more sheets than this (the scale is reduced instead).
pub const MAX_PAGES: usize = 64;
/// Height of the running header (title and page number), in mm.
const HEADER_MM: f64 = 8.0;
/// One drawing unit (a CSS pixel) in mm.
const MM_PER_UNIT: f64 = 25.4 / 96.0;
const FONT: &str = "DejaVu Sans, Arial, Helvetica, sans-serif";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PageSize {
    A5,
    #[default]
    A4,
    A3,
    Letter,
    Legal,
    Tabloid,
}

impl PageSize {
    pub const ALL: [PageSize; 6] = [
        PageSize::A5,
        PageSize::A4,
        PageSize::A3,
        PageSize::Letter,
        PageSize::Legal,
        PageSize::Tabloid,
    ];

    pub fn name(self) -> &'static str {
        match self {
            PageSize::A5 => "A5",
            PageSize::A4 => "A4",
            PageSize::A3 => "A3",
            PageSize::Letter => "Letter",
            PageSize::Legal => "Legal",
            PageSize::Tabloid => "Tabloid",
        }
    }

    /// Portrait width and height in mm.
    pub fn portrait_mm(self) -> (f64, f64) {
        match self {
            PageSize::A5 => (148.0, 210.0),
            PageSize::A4 => (210.0, 297.0),
            PageSize::A3 => (297.0, 420.0),
            PageSize::Letter => (215.9, 279.4),
            PageSize::Legal => (215.9, 355.6),
            PageSize::Tabloid => (279.4, 431.8),
        }
    }

    pub fn parse(s: &str) -> Option<PageSize> {
        PageSize::ALL
            .into_iter()
            .find(|p| p.name().eq_ignore_ascii_case(s.trim()))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Orientation {
    #[default]
    Landscape,
    Portrait,
}

impl Orientation {
    pub fn name(self) -> &'static str {
        match self {
            Orientation::Landscape => "landscape",
            Orientation::Portrait => "portrait",
        }
    }
    pub fn parse(s: &str) -> Option<Orientation> {
        match s.trim().to_ascii_lowercase().as_str() {
            "landscape" => Some(Orientation::Landscape),
            "portrait" => Some(Orientation::Portrait),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Fit {
    /// Scale the whole drawing onto one sheet.
    #[default]
    FitToPage,
    /// Keep the drawing at `scale_percent` and spread it over as many sheets as needed.
    SplitPages,
}

impl Fit {
    pub fn name(self) -> &'static str {
        match self {
            Fit::FitToPage => "fit",
            Fit::SplitPages => "split",
        }
    }
    pub fn parse(s: &str) -> Option<Fit> {
        match s.trim().to_ascii_lowercase().as_str() {
            "fit" | "fit-to-page" => Some(Fit::FitToPage),
            "split" | "pages" => Some(Fit::SplitPages),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageSetup {
    pub size: PageSize,
    pub orientation: Orientation,
    pub fit: Fit,
    /// Margin on every side, mm (clamped to 0..=40).
    pub margin_mm: f64,
    /// Drawing scale when splitting, percent of natural size (clamped to 10..=400).
    pub scale_percent: f64,
}

impl Default for PageSetup {
    fn default() -> Self {
        Self {
            size: PageSize::A4,
            orientation: Orientation::Landscape,
            fit: Fit::FitToPage,
            margin_mm: 12.0,
            scale_percent: 100.0,
        }
    }
}

impl PageSetup {
    /// Sheet width and height in mm after orientation.
    pub fn sheet_mm(&self) -> (f64, f64) {
        let (w, h) = self.size.portrait_mm();
        match self.orientation {
            Orientation::Portrait => (w, h),
            Orientation::Landscape => (h, w),
        }
    }

    /// The margin, sanitised, for callers laying out text around the sheets.
    pub fn content_margin(&self) -> f64 {
        if self.margin_mm.is_finite() {
            self.margin_mm.clamp(0.0, 40.0)
        } else {
            12.0
        }
    }

    fn scale(&self) -> f64 {
        if self.scale_percent.is_finite() {
            self.scale_percent.clamp(10.0, 400.0)
        } else {
            100.0
        }
    }

    /// Printable area (inside margins, below the header) in mm.
    pub fn content_mm(&self) -> (f64, f64) {
        let (w, h) = self.sheet_mm();
        let m = self.content_margin();
        ((w - 2.0 * m).max(10.0), (h - 2.0 * m - HEADER_MM).max(10.0))
    }
}

/// Width and height of an `<svg width=".." height="..">` root tag.
fn svg_size(svg: &str) -> (f64, f64) {
    let head = svg.split('>').next().unwrap_or("");
    let attr = |name: &str| {
        let key = format!(" {name}=\"");
        let start = head.find(&key)? + key.len();
        let end = head.get(start..)?.find('"')? + start;
        head.get(start..end)?.parse::<f64>().ok()
    };
    let ok = |v: Option<f64>| v.filter(|v| v.is_finite() && *v > 0.0).unwrap_or(1.0);
    (ok(attr("width")), ok(attr("height")))
}

/// Sheets across, sheets down, and the scale in mm per drawing unit.
pub fn page_grid(setup: &PageSetup, drawing: (f64, f64)) -> (usize, usize, f64) {
    let (cw, ch) = setup.content_mm();
    let (w, h) = drawing;
    match setup.fit {
        Fit::FitToPage => (1, 1, (cw / w).min(ch / h).min(2.0 * MM_PER_UNIT)),
        Fit::SplitPages => {
            let mut s = MM_PER_UNIT * setup.scale() / 100.0;
            loop {
                let cols = ((w * s / cw).ceil() as usize).max(1);
                let rows = ((h * s / ch).ceil() as usize).max(1);
                if cols.saturating_mul(rows) <= MAX_PAGES {
                    return (cols, rows, s);
                }
                s *= 0.9;
            }
        }
    }
}

/// How many sheets the tree needs under `setup`.
pub fn page_count(
    tree: &Tree,
    db: &PrimaryEventDb,
    probabilities: bool,
    setup: &PageSetup,
) -> usize {
    let svg = tree_svg(
        tree,
        db,
        &SvgOptions {
            title: String::new(),
            probabilities,
            background: false,
        },
    );
    let (c, r, _) = page_grid(setup, svg_size(&svg));
    c * r
}

/// The tree on paper: one SVG (sized in mm) per sheet.
pub fn print_pages(
    tree: &Tree,
    db: &PrimaryEventDb,
    title: &str,
    probabilities: bool,
    setup: &PageSetup,
) -> Vec<String> {
    print_pages_with(
        tree,
        db,
        title,
        probabilities,
        setup,
        &Preferences::default(),
    )
}

/// Like [`print_pages`], with the user's colours and monochrome mode.
pub fn print_pages_with(
    tree: &Tree,
    db: &PrimaryEventDb,
    title: &str,
    probabilities: bool,
    setup: &PageSetup,
    prefs: &Preferences,
) -> Vec<String> {
    let svg = tree_svg_with(
        tree,
        db,
        &SvgOptions {
            title: String::new(),
            probabilities,
            background: false,
        },
        prefs,
    );
    let drawing = svg_size(&svg);
    let (cols, rows, s) = page_grid(setup, drawing);
    let (sw, sh) = setup.sheet_mm();
    let m = setup.content_margin();
    let (cw, ch) = setup.content_mm();
    let total = cols * rows;
    let title = xml_escape(title);
    let mut pages = Vec::with_capacity(total);
    for r in 0..rows {
        for c in 0..cols {
            let n = r * cols + c + 1;
            // drawing-unit window shown on this sheet
            let (tw, th) = (cw / s, ch / s);
            let (tx, ty) = (c as f64 * tw, r as f64 * th);
            let vw = tw.min(drawing.0 - tx).max(0.0);
            let vh = th.min(drawing.1 - ty).max(0.0);
            let (ox, oy) = if setup.fit == Fit::FitToPage {
                (m + (cw - vw * s) / 2.0, m + HEADER_MM)
            } else {
                (m, m + HEADER_MM)
            };
            let mut p = String::new();
            p.push_str(&format!(
                "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{sw:.1}mm\" height=\"{sh:.1}mm\" viewBox=\"0 0 {sw:.1} {sh:.1}\" font-family=\"{FONT}\">\n"
            ));
            p.push_str(&format!(
                "<rect width=\"{sw:.1}\" height=\"{sh:.1}\" fill=\"#ffffff\"/>\n"
            ));
            p.push_str(&format!(
                "<text x=\"{m:.1}\" y=\"{:.1}\" font-size=\"4\" font-weight=\"bold\">{title}</text>\n",
                m + 4.5
            ));
            if total > 1 {
                p.push_str(&format!(
                    "<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"3\" text-anchor=\"end\" fill=\"#555555\">Page {n} of {total} (row {}, column {})</text>\n",
                    sw - m,
                    m + 4.5,
                    r + 1,
                    c + 1
                ));
            }
            if vw > 0.0 && vh > 0.0 {
                p.push_str(&format!(
                    "<svg x=\"{ox:.3}\" y=\"{oy:.3}\" width=\"{:.3}\" height=\"{:.3}\" viewBox=\"{tx:.3} {ty:.3} {vw:.3} {vh:.3}\" overflow=\"hidden\">\n",
                    vw * s,
                    vh * s
                ));
                p.push_str(&svg);
                p.push_str("\n</svg>\n");
            }
            if total > 1 {
                p.push_str(&format!(
                    "<rect x=\"{m:.1}\" y=\"{:.1}\" width=\"{cw:.1}\" height=\"{ch:.1}\" fill=\"none\" stroke=\"#bbbbbb\" stroke-width=\"0.2\" stroke-dasharray=\"2 2\"/>\n",
                    m + HEADER_MM
                ));
            }
            p.push_str("</svg>\n");
            pages.push(p);
        }
    }
    pages
}

impl PageSetup {
    /// Build a setup from optional textual options (CLI flags, tool arguments).
    /// Unset options keep their defaults; unknown values are an error naming the choices.
    pub fn from_options(
        size: Option<&str>,
        orientation: Option<&str>,
        fit: Option<&str>,
        scale_percent: Option<f64>,
        margin_mm: Option<f64>,
    ) -> Result<PageSetup, String> {
        let mut p = PageSetup::default();
        if let Some(s) = size {
            p.size = PageSize::parse(s).ok_or_else(|| {
                format!("unknown page size `{s}` (A5, A4, A3, Letter, Legal, Tabloid)")
            })?;
        }
        if let Some(s) = orientation {
            p.orientation = Orientation::parse(s)
                .ok_or_else(|| format!("unknown orientation `{s}` (landscape, portrait)"))?;
        }
        if let Some(s) = fit {
            p.fit = Fit::parse(s).ok_or_else(|| format!("unknown fit `{s}` (fit, split)"))?;
        }
        if let Some(v) = scale_percent {
            p.scale_percent = v;
        }
        if let Some(v) = margin_mm {
            p.margin_mm = v;
        }
        Ok(p)
    }
}
