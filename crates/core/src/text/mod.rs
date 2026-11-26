//! A plain-text fault tree format (`.ftt`), written by hand and diffed in git,
//! and compatible with [PFTA](https://github.com/public-fta/pfta).
//!
//! The syntax is PFTA's: paragraphs that start with `Gate: ID`, `Event: ID`
//! or `Model: ID` and go on with `- key: value` lines, after an optional
//! paragraph of tree properties. ftar reads PFTA's files (time-dependent
//! models, distributions, VOTE gates, several top gates) and writes files
//! PFTA reads, as long as the tree only uses what PFTA has. OpenFTA's extra
//! gate and event types and its other data are ftar keys on top. See
//! `docs/TEXT-FORMAT.md` for the whole format.
//!
//! ```text
//! - times: 1, 10, 100
//!
//! Gate: Top
//! - label: Electrical power to operating theatre fails.
//! - type: AND
//! - inputs: Backup_Fail, MAINS_FAIL
//!
//! Gate: Backup_Fail
//! - type: OR
//! - inputs: RELAY_FAIL, GEN_FAIL
//!
//! Event: GEN_FAIL
//! - model_type: ConstantRate
//! - failure_rate: 0.02
//! - repair_rate: 0
//!
//! Event: RELAY_FAIL
//! - model_type: Fixed
//! - probability: 0.05
//! - intensity: 0
//!
//! Event: MAINS_FAIL
//! - model_type: Fixed
//! - probability: 0.03
//! - intensity: 0
//! ```
//!
//! A file is read in two steps: [`ast::parse_ast`] checks every paragraph,
//! [`convert::to_document`] builds ftar's tree (from the first top gate),
//! event database and [`Extras`]. Writing goes the other way.

pub mod ast;
pub mod convert;

use std::path::Path;

pub use ast::{
    is_pfta_id, Ast, Distribution, EventDef, EventModel, GateDef, GateType, Model, ModelDef, Param,
    Props, Rate,
};
pub use convert::{EventExtra, Extras, GateExtra, MAX_NODES};

use crate::error::{Error, Result};
use crate::model::{PrimaryEventDb, Tree};

/// File extension of the text format (without the dot).
pub const EXTENSION: &str = "ftt";

/// Does `path` name a text-format tree (by its extension)?
pub fn is_text_path(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case(EXTENSION))
}

/// A text file read into ftar's tree, event database and extras.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TextDocument {
    /// The tree under the first top gate (without a database name).
    pub tree: Tree,
    pub db: PrimaryEventDb,
    pub extras: Extras,
}

/// Read a text-format file.
pub fn parse_text(input: &[u8]) -> Result<TextDocument> {
    let ast = parse_text_ast(input)?;
    let (tree, db, extras) = convert::to_document(&ast)?;
    Ok(TextDocument { tree, db, extras })
}

/// Read a text-format file into its paragraphs only.
pub fn parse_text_ast(input: &[u8]) -> Result<Ast> {
    let text = std::str::from_utf8(input)
        .map_err(|e| Error::Parse(format!("the file is not UTF-8: {e}")))?;
    ast::parse_ast(text)
}

/// Write a tree, its event database and extras in the text format.
/// [`parse_text`] reads the result back to the same tree and database,
/// except that markers other than the dormant `D` and the rate `L` (OpenFTA
/// also wrote spaces there) become absent. Fails with
/// [`Error::Unsupported`] for trees the format cannot hold: structures
/// OpenFTA does not allow (a gate directly under a gate), one event id with
/// two types, or ids with spaces or `,`.
pub fn write_text(tree: &Tree, db: &PrimaryEventDb, extras: &Extras) -> Result<String> {
    Ok(ast::render(&convert::from_document(tree, db, extras)?))
}

/// Is a file's content something PFTA reads? Its own gate, event and model
/// kinds only, ids of letters, digits, `_` and `-`, and none of ftar's keys.
/// Returns what stands in the way.
pub fn pfta_obstacles(ast: &Ast) -> Vec<String> {
    use crate::model::EventType;
    let mut out = Vec::new();
    let p = &ast.props;
    if p.note.is_some() {
        out.push("the tree has a note".to_string());
    }
    if p.transfer_out.is_some() {
        out.push("the tree has a transfer-out".to_string());
    }
    if p.times.is_none() {
        out.push("no `times`".to_string());
    }
    let ids = ast
        .models
        .iter()
        .map(|m| &m.id)
        .chain(ast.gates.iter().map(|g| &g.id))
        .chain(ast.events.iter().map(|e| &e.id));
    for id in ids {
        if !is_pfta_id(id) {
            out.push(format!("id `{id}` has characters PFTA does not allow"));
        }
    }
    for g in &ast.gates {
        if !g.gate_type.is_pfta() {
            out.push(format!("gate `{}` is {}", g.id, g.gate_type.name()));
        } else if g.gate_type == GateType::Null && g.inputs.len() != 1 {
            out.push(format!("gate `{}` has no type", g.id));
        }
        if g.gate_id.is_some() || g.node_id.is_some() {
            out.push(format!("gate `{}` has OpenFTA ids", g.id));
        }
        if g.inputs.is_empty() {
            out.push(format!("gate `{}` has no inputs", g.id));
        }
    }
    for e in &ast.events {
        if !matches!(
            e.kind,
            EventType::Basic | EventType::Undeveloped | EventType::External
        ) {
            out.push(format!("event `{}` has an OpenFTA type", e.id));
        }
        if e.dormant || e.group.is_some() {
            out.push(format!("event `{}` has OpenFTA data", e.id));
        }
        if e.model.is_none() {
            out.push(format!("event `{}` has no model", e.id));
        }
    }
    out
}
