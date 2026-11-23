//! `ftar-render` - headless drawing of fault trees.
//!
//! * [`scene`] - the tree laid out and drawn as OpenFTA draws it, as shapes and text
//!   that the SVG export and the GUI canvas both draw
//! * [`svg`]  - the tree as a self-contained SVG (vector, printable, scalable)
//! * [`png`]  - the same drawing rasterised with resvg
//! * [`html`] - a printable page (title, note, drawing, optional report text) that can
//!   open the browser's print dialog
//!
//! * [`page`] - page size and orientation, fit-to-page or split across sheets (print preview)
//!
//! Everything is pure: no windows, no GPU, no file I/O.

pub mod html;
pub mod page;
pub mod png;
pub mod prefs;
pub mod scene;
pub mod svg;

pub use html::{print_html, print_html_with, report_html, PrintOptions};
pub use page::{
    page_count, page_grid, print_pages, print_pages_with, Fit, Orientation, PageSetup, PageSize,
    MAX_PAGES,
};
pub use png::{svg_to_png, svg_to_rgba, RenderError};
pub use prefs::{Preferences, Symbol, Theme};
pub use scene::{tree_scene, Scene, SceneOptions};
pub use svg::{tree_svg, tree_svg_with, SvgOptions};
