//! `ftar-render` - headless drawing of fault trees.
//!
//! * [`scene`] - the tree laid out and drawn as OpenFTA draws it, as shapes and text
//!   that the SVG export and the GUI canvas both draw
//! * [`svg`]  - the tree as a self-contained SVG (vector, printable, scalable)
//! * [`png`]  - the same drawing rasterised with resvg
//! * [`html`] - a printable page (title, note, drawing, optional report text) that can
//!   open the browser's print dialog
//!
//! Everything is pure: no windows, no GPU, no file I/O.

pub mod html;
pub mod png;
pub mod scene;
pub mod svg;

pub use html::{print_html, PrintOptions};
pub use png::{svg_to_png, RenderError};
pub use scene::{tree_scene, Scene, SceneOptions};
pub use svg::{tree_svg, SvgOptions};
