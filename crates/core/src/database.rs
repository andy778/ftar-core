//! Editing the primary event database (`.ped`), as OpenPED does: add or
//! change an event, remove one. Headless, so the GUI, the CLI tools and tests
//! all go through the same checks.

use crate::edit::{EditError, EditResult};
use crate::model::{EventType, PrimaryEvent, PrimaryEventDb, Tree};
use crate::project::apply_ped_types;

/// Longest description or id accepted; keeps files and reports sane.
pub const MAX_TEXT: usize = 4096;

fn is_database_kind(k: EventType) -> bool {
    use EventType::*;
    matches!(
        k,
        Basic | External | Undeveloped | ConditionAnalysed | ConditionNotAnalysed
    )
}

/// Check one event the way the `.ped` format needs it: an id without spaces or
/// `;`, a single-line description without `;`, a type a database can hold, a
/// dormant marker that is absent or `D`, and a probability within 0..1
/// (any non-negative rate when the event is a failure rate).
pub fn validate_event(ev: &PrimaryEvent) -> EditResult<()> {
    if ev.id.is_empty()
        || ev.id.len() > MAX_TEXT
        || ev
            .id
            .chars()
            .any(|c| c.is_whitespace() || c == ';' || c.is_control())
    {
        return Err(EditError::InvalidText(
            "an event id is required and cannot contain spaces or `;`",
        ));
    }
    if ev.description.len() > MAX_TEXT || ev.description.chars().any(|c| c == ';' || c.is_control())
    {
        return Err(EditError::InvalidText(
            "a description must be a single line without `;`",
        ));
    }
    if !is_database_kind(ev.kind) {
        return Err(EditError::InvalidText(
            "an event type must be basic, external, undeveloped or a condition",
        ));
    }
    if !matches!(ev.modifier, None | Some('D')) {
        return Err(EditError::InvalidText("the dormant marker must be `D`"));
    }
    if !matches!(ev.lambda, None | Some('L')) {
        return Err(EditError::InvalidText("the rate marker must be `L`"));
    }
    let rate = ev.lambda == Some('L');
    if !ev.probability.is_finite() || ev.probability < 0.0 || (!rate && ev.probability > 1.0) {
        return Err(EditError::InvalidText(
            "probability must be within 0..1 (a failure rate must be >= 0)",
        ));
    }
    Ok(())
}

/// Add the event or replace the one with the same id. Nodes of `tree` that use
/// the id take on the event's type, as when a tree is loaded. Nothing changes
/// when the event is invalid.
pub fn upsert_event(tree: &mut Tree, db: &mut PrimaryEventDb, ev: PrimaryEvent) -> EditResult<()> {
    validate_event(&ev)?;
    db.events.insert(ev.id.clone(), ev);
    apply_ped_types(&mut tree.root, db);
    Ok(())
}

/// Remove an event from the database. Returns how many nodes of `tree` still
/// use the id (they will have no data until it is added again), or `None`
/// when the database had no such event.
pub fn remove_event(tree: &Tree, db: &mut PrimaryEventDb, id: &str) -> Option<usize> {
    db.events.remove(id)?;
    Some(tree.nodes().iter().filter(|n| n.id == id).count())
}

/// Text report of a database, as OpenPED's File > Export Text File writes it:
/// a header with the database name (`Unnamed` when empty), then one block per
/// event (sorted by id) with id, probability, type, dormant flag and
/// description, blocks separated by a blank line. Probabilities print in the
/// shortest `f32` form, as Java's `Float.toString` does for ordinary values.
pub fn export_text(db: &PrimaryEventDb, name: &str) -> String {
    let mut out = String::from("Primary Events Database\n=======================\n\n");
    out.push_str("Database Filename: ");
    out.push_str(if name.is_empty() { "Unnamed" } else { name });
    out.push_str("\n\n");
    let mut ids: Vec<&String> = db.events.keys().collect();
    ids.sort();
    let blocks: Vec<String> = ids
        .into_iter()
        .map(|id| {
            let e = &db.events[id];
            let mut p = format!("{:?}", e.probability as f32);
            if p == "inf" || p == "NaN" {
                p = "0.0".into();
            }
            let rate = if e.lambda == Some('L') {
                " (Constant failure rate/Unit time)"
            } else {
                ""
            };
            format!(
                "ID: {}\nProbability: {p}{rate}\nType: {}\nDormant: {}\nDescription: {}",
                e.id,
                type_name(e.kind),
                if e.modifier == Some('D') { "Yes" } else { "No" },
                e.description
            )
        })
        .collect();
    out.push_str(&blocks.join("\n\n"));
    out
}

fn type_name(k: EventType) -> &'static str {
    match k {
        EventType::External => "External",
        EventType::Undeveloped => "Undeveloped",
        EventType::ConditionNotAnalysed => "ConditionNotAnalysed",
        EventType::ConditionAnalysed => "ConditionAnalysed",
        _ => "Basic",
    }
}

/// What [`copy_events`] did.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct CopyOutcome {
    /// Ids added or replaced in the destination.
    pub copied: Vec<String>,
    /// Ids left alone because the destination had them and `overwrite` was off.
    pub skipped: Vec<String>,
    /// Ids not found in the source.
    pub missing: Vec<String>,
}

/// Copy events by id from one database to another (OpenPED copy/paste between
/// databases). Existing ids are replaced only when `overwrite`. Events are
/// validated first; the destination is untouched when one is invalid.
pub fn copy_events(
    src: &PrimaryEventDb,
    dst: &mut PrimaryEventDb,
    ids: &[String],
    overwrite: bool,
) -> EditResult<CopyOutcome> {
    let mut out = CopyOutcome::default();
    let mut add = Vec::new();
    for id in ids {
        match src.events.get(id) {
            None => out.missing.push(id.clone()),
            Some(_) if dst.events.contains_key(id) && !overwrite => out.skipped.push(id.clone()),
            Some(e) => {
                validate_event(e)?;
                add.push(e.clone());
                out.copied.push(id.clone());
            }
        }
    }
    for e in add {
        dst.events.insert(e.id.clone(), e);
    }
    Ok(out)
}

/// Result of [`link_node`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkOutcome {
    Linked,
    /// The node's type differs from the event's; nothing changed. OpenFTA
    /// asks the user; call again with `force` to link anyway.
    TypesDiffer,
}

/// Edit > Link: make the leaf event at `path` use database event `id`. The
/// node takes the event's id and type (as a loaded tree does).
pub fn link_node(
    tree: &mut Tree,
    db: &PrimaryEventDb,
    path: &[usize],
    id: &str,
    force: bool,
) -> EditResult<LinkOutcome> {
    let ev = db
        .events
        .get(id)
        .ok_or(EditError::InvalidText("no such event in the database"))?;
    let node = crate::edit::node_at(tree, path).ok_or(EditError::InvalidPath)?;
    if !node.children.is_empty() || !is_database_kind(node.kind) {
        return Err(EditError::InvalidText("select a primary event to link"));
    }
    if node.kind != ev.kind && !force {
        return Ok(LinkOutcome::TypesDiffer);
    }
    let (kind, modifier) = (ev.kind, ev.modifier);
    crate::edit::set_id(tree, path, id)?;
    crate::edit::change_type(tree, path, kind, modifier)?;
    Ok(LinkOutcome::Linked)
}
