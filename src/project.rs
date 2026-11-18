//! Loading a tree from disk together with its primary-event database and
//! any transfer-in trees (which are separate `.fta` files in the same directory).

use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::model::{EventType, Node, PrimaryEventDb, Tree};
use crate::parse::{parse_fta, parse_ped};

#[derive(Debug, Clone)]
pub struct Project {
    /// Tree with all transfer-ins inlined.
    pub tree: Tree,
    /// Primary events of the tree and of all transferred trees.
    pub db: PrimaryEventDb,
}

impl Project {
    pub fn load(path: impl AsRef<Path>) -> Result<Project> {
        let mut db = PrimaryEventDb::default();
        let mut stack = Vec::new();
        let mut tree = load_tree(path.as_ref(), &mut db, &mut stack)?;
        // Transfer-out gates only mark where a subtree is used elsewhere.
        strip_transfer_out(&mut tree.root);
        Ok(Project { tree, db })
    }
}

fn read(path: &Path) -> Result<Vec<u8>> {
    std::fs::read(path).map_err(|e| Error::Io(format!("{}: {e}", path.display())))
}

fn load_tree(path: &Path, db: &mut PrimaryEventDb, stack: &mut Vec<PathBuf>) -> Result<Tree> {
    let canon = path.canonicalize().map_err(|e| Error::Io(format!("{}: {e}", path.display())))?;
    if stack.contains(&canon) {
        return Err(Error::Parse(format!("transfer-in cycle through {}", path.display())));
    }
    stack.push(canon);

    let mut tree = parse_fta(&read(path)?)?;
    let dir = path.parent().unwrap_or_else(|| Path::new("."));

    let mut own_ped = None;
    if let Some(name) = &tree.db_name {
        // Old files may store an absolute (Windows) path; fall back to the file name.
        let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
        let candidates = [dir.join(name), dir.join(base)];
        if let Some(p) = candidates.iter().find(|p| p.is_file()) {
            let ped = parse_ped(&read(p)?)?;
            for (k, v) in &ped.events {
                db.events.entry(k.clone()).or_insert_with(|| v.clone());
            }
            own_ped = Some(ped);
        }
    }
    // As in OpenFTA's `load_item`, the event type recorded in the tree's own
    // database overrides the type letter stored in the `.fta` file.
    if let Some(ped) = &own_ped {
        apply_ped_types(&mut tree.root, ped);
    }

    inline_transfers(&mut tree.root, dir, db, stack)?;
    stack.pop();
    Ok(tree)
}

fn apply_ped_types(n: &mut Node, ped: &PrimaryEventDb) {
    use EventType::*;
    if n.kind.is_primary() || matches!(n.kind, ConditionAnalysed | ConditionNotAnalysed) {
        if let Some(e) = ped.events.get(&n.id) {
            if matches!(e.kind, Basic | External | Undeveloped | ConditionAnalysed | ConditionNotAnalysed) {
                n.kind = e.kind;
            }
        }
    }
    for c in &mut n.children {
        apply_ped_types(c, ped);
    }
}

fn inline_transfers(
    n: &mut Node,
    dir: &Path,
    db: &mut PrimaryEventDb,
    stack: &mut Vec<PathBuf>,
) -> Result<()> {
    if n.kind == EventType::TransferIn {
        let sub = load_tree(&dir.join(&n.id), db, stack)?;
        n.kind = EventType::Intermediate;
        n.children = vec![sub.root];
        return Ok(());
    }
    for c in &mut n.children {
        inline_transfers(c, dir, db, stack)?;
    }
    Ok(())
}

fn strip_transfer_out(n: &mut Node) {
    n.children.retain(|c| c.kind != EventType::TransferOut);
    for c in &mut n.children {
        strip_transfer_out(c);
    }
}
