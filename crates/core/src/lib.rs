//! `ftar-core` — fault tree analysis core, a Rust port of the OpenFTA engine.
//!
//! * [`model`]   – tree and event types
//! * [`parse`]   – readers for OpenFTA `.fta` (tree) and `.ped` (primary event database) files
//! * [`project`]  – load a tree with its database and transfer-in trees
//! * [`analysis`] – minimal cut sets, exact top-event probability, importance
//! * [`montecarlo`] – Monte Carlo simulation

pub mod analysis;
pub mod edit;
pub mod error;
pub mod layout;
pub mod model;
pub mod montecarlo;
pub mod parse;
pub mod project;
pub mod report;
pub mod write;

pub use error::{Error, Result};
pub use model::{EventType, Node, PrimaryEvent, PrimaryEventDb, Tree};
