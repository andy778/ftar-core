//! `ftar-core` — fault tree analysis core, a Rust port of the OpenFTA engine.
//!
//! * [`model`]   – tree and event types
//! * [`parse`]   – readers for OpenFTA `.fta` (tree) and `.ped` (primary event database) files
//! * [`project`]  – load a tree with its database and transfer-in trees
//! * [`analysis`] – minimal cut sets, exact top-event probability, importance
//! * [`montecarlo`] – Monte Carlo simulation
//! * [`text`]     – the plain-text tree format (`.ftt`, PFTA-compatible), tree and database in one file
//! * [`quant`]    – time-dependent quantification with uncertainty (PFTA's analysis)

pub mod analysis;
pub mod database;
pub mod edit;
pub mod error;
pub mod model;
pub mod montecarlo;
pub mod parse;
pub mod project;
pub mod quant;
pub mod report;
pub mod report_files;
pub mod text;
pub mod write;

pub use error::{Error, Result};
pub use model::{EventType, Node, PrimaryEvent, PrimaryEventDb, Tree};
