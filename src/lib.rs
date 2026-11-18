//! `ftar-core` — fault tree analysis core, a Rust port of the OpenFTA engine.
//!
//! * [`model`]   – tree and event types
//! * [`parse`]   – readers for OpenFTA `.fta` (tree) and `.ped` (primary event database) files
//! * [`analysis`] – minimal cut sets and top-event probability

pub mod analysis;
pub mod error;
pub mod model;
pub mod parse;

pub use error::{Error, Result};
pub use model::{EventType, Node, PrimaryEvent, PrimaryEventDb, Tree};
