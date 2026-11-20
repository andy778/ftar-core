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
