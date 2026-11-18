//! Loading a tree from disk together with its primary-event database and
//! any transfer-in trees (which are separate `.fta` files in the same directory).

use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::model::{EventType, Node, PrimaryEventDb, Tree, MAX_DEPTH};
use crate::parse::{parse_fta, parse_ped};

/// Limits for loading files named *inside* a tree file (its database and its
/// transfer-in trees), for callers that serve untrusted input.
#[derive(Debug, Clone, Default)]
pub struct LoadOptions {
    /// Every file read must resolve (after following links) to a path inside
    /// this directory. Database names that point outside fall back to the file
    /// name next to the tree, as for old files with absolute paths; transfer-ins
    /// that point outside are an error.
    pub root: Option<PathBuf>,
    /// Refuse files larger than this many bytes.
    pub max_file_size: Option<u64>,
}

impl LoadOptions {
    /// Confine reads to `root` (which is canonicalised here).
    pub fn confined(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref();
        let root = root
            .canonicalize()
            .map_err(|e| Error::Io(format!("{}: {e}", root.display())))?;
        Ok(Self {
            root: Some(root),
            max_file_size: None,
        })
    }

    /// Is `p` an existing file that may be read under these options?
    fn allows(&self, p: &Path) -> bool {
        match &self.root {
            None => p.is_file(),
            Some(root) => p
                .canonicalize()
                .is_ok_and(|c| c.is_file() && c.starts_with(root)),
        }
    }

    fn read(&self, path: &Path) -> Result<Vec<u8>> {
        if let Some(root) = &self.root {
            let c = path
                .canonicalize()
                .map_err(|e| Error::Io(format!("{}: {e}", path.display())))?;
            if !c.starts_with(root) {
                return Err(Error::Io(format!(
                    "{}: outside the allowed directory",
                    path.display()
                )));
            }
        }
        if let Some(max) = self.max_file_size {
            let len = std::fs::metadata(path)
                .map_err(|e| Error::Io(format!("{}: {e}", path.display())))?
                .len();
            if len > max {
                return Err(Error::Io(format!("{}: file too large", path.display())));
            }
        }
        read(path)
    }

    /// The `.ped` file a tree in `dir` refers to by `name`, if it exists and may be read.
    fn find_ped(&self, dir: &Path, name: &str) -> Option<PathBuf> {
        // Old files may store an absolute (Windows) path; fall back to the file name.
        let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
        [dir.join(name), dir.join(base)]
            .into_iter()
            .find(|p| self.allows(p))
    }
}

#[derive(Debug, Clone)]
pub struct Project {
    /// Tree with all transfer-ins inlined.
    pub tree: Tree,
    /// Primary events of the tree and of all transferred trees.
    pub db: PrimaryEventDb,
}

impl Project {
    pub fn load(path: impl AsRef<Path>) -> Result<Project> {
        Self::load_with(path, &LoadOptions::default())
    }

    /// Like [`Project::load`], under the limits in `opts`.
    pub fn load_with(path: impl AsRef<Path>, opts: &LoadOptions) -> Result<Project> {
        let mut db = PrimaryEventDb::default();
        let mut stack = Vec::new();
        let mut tree = load_tree(path.as_ref(), &mut db, &mut stack, opts)?;
        // Transfer-out gates only mark where a subtree is used elsewhere.
        strip_transfer_out(&mut tree.root);
        check_depth(&tree)?;
        Ok(Project { tree, db })
    }
}

fn read(path: &Path) -> Result<Vec<u8>> {
    std::fs::read(path).map_err(|e| Error::Io(format!("{}: {e}", path.display())))
}

/// Inlined transfer-ins can nest trees that are each within the limit.
fn check_depth(tree: &Tree) -> Result<()> {
    if tree.root.depth() > MAX_DEPTH {
        return Err(Error::Parse(format!(
            "tree (with transfers inlined) is nested more than {MAX_DEPTH} levels deep"
        )));
    }
    Ok(())
}

fn load_tree(
    path: &Path,
    db: &mut PrimaryEventDb,
    stack: &mut Vec<PathBuf>,
    opts: &LoadOptions,
) -> Result<Tree> {
    let canon = path
        .canonicalize()
        .map_err(|e| Error::Io(format!("{}: {e}", path.display())))?;
    if stack.contains(&canon) {
        return Err(Error::Parse(format!(
            "transfer-in cycle through {}",
            path.display()
        )));
    }
    if stack.len() >= MAX_DEPTH {
        return Err(Error::Parse("transfer-ins nested too deep".into()));
    }
    stack.push(canon);

    let mut tree = parse_fta(&opts.read(path)?)?;
    let dir = path.parent().unwrap_or_else(|| Path::new("."));

    let mut own_ped = None;
    if let Some(p) = tree.db_name.as_deref().and_then(|n| opts.find_ped(dir, n)) {
        let ped = parse_ped(&opts.read(&p)?)?;
        for (k, v) in &ped.events {
            db.events.entry(k.clone()).or_insert_with(|| v.clone());
        }
        own_ped = Some(ped);
    }
    // As in OpenFTA's `load_item`, the event type recorded in the tree's own
    // database overrides the type letter stored in the `.fta` file.
    if let Some(ped) = &own_ped {
        apply_ped_types(&mut tree.root, ped);
    }

    inline_transfers(&mut tree.root, dir, db, stack, opts)?;
    stack.pop();
    Ok(tree)
}

fn apply_ped_types(n: &mut Node, ped: &PrimaryEventDb) {
    use EventType::*;
    if n.kind.is_primary() || matches!(n.kind, ConditionAnalysed | ConditionNotAnalysed) {
        if let Some(e) = ped.events.get(&n.id) {
            if matches!(
                e.kind,
                Basic | External | Undeveloped | ConditionAnalysed | ConditionNotAnalysed
            ) {
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
    opts: &LoadOptions,
) -> Result<()> {
    if n.kind == EventType::TransferIn {
        let sub = load_tree(&dir.join(&n.id), db, stack, opts)?;
        n.kind = EventType::Intermediate;
        n.children = vec![sub.root];
        return Ok(());
    }
    for c in &mut n.children {
        inline_transfers(c, dir, db, stack, opts)?;
    }
    Ok(())
}

fn strip_transfer_out(n: &mut Node) {
    n.children.retain(|c| c.kind != EventType::TransferOut);
    for c in &mut n.children {
        strip_transfer_out(c);
    }
}

/// An editable document: the tree exactly as stored in its file (transfer-in
/// gates are *not* expanded) plus its own primary-event database.
///
/// Use [`Document::analysis_project`] to get the expanded tree and merged
/// database that the analyses need.
#[derive(Debug, Clone, Default)]
pub struct Document {
    pub path: Option<PathBuf>,
    pub tree: Tree,
    pub db: PrimaryEventDb,
}

impl Default for Tree {
    fn default() -> Self {
        let mut top = Node {
            kind: EventType::Intermediate,
            modifier: None,
            id: String::new(),
            text: String::new(),
            children: Vec::new(),
        };
        top.children.push(Node {
            kind: EventType::Or,
            modifier: None,
            id: String::new(),
            text: String::new(),
            children: Vec::new(),
        });
        Tree {
            db_name: None,
            note: String::new(),
            root: top,
        }
    }
}

impl Document {
    /// Load a tree and its own `.ped` without following transfers.
    pub fn load(path: impl AsRef<Path>) -> Result<Document> {
        Self::load_with(path, &LoadOptions::default())
    }

    /// Like [`Document::load`], under the limits in `opts`.
    pub fn load_with(path: impl AsRef<Path>, opts: &LoadOptions) -> Result<Document> {
        let path = path.as_ref();
        let mut tree = parse_fta(&opts.read(path)?)?;
        let dir = path.parent().unwrap_or_else(|| Path::new("."));
        let mut db = PrimaryEventDb::default();
        if let Some(p) = tree.db_name.as_deref().and_then(|n| opts.find_ped(dir, n)) {
            db = parse_ped(&opts.read(&p)?)?;
            apply_ped_types(&mut tree.root, &db);
        }
        Ok(Document {
            path: Some(path.to_path_buf()),
            tree,
            db,
        })
    }

    /// Directory transfer-in files are resolved against.
    pub fn dir(&self) -> PathBuf {
        self.path
            .as_deref()
            .and_then(Path::parent)
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."))
    }

    /// Expand transfers and merge the databases of transferred trees, ready for analysis.
    pub fn analysis_project(&self) -> Result<Project> {
        self.analysis_project_with(&LoadOptions::default())
    }

    /// Like [`Document::analysis_project`], under the limits in `opts`.
    pub fn analysis_project_with(&self, opts: &LoadOptions) -> Result<Project> {
        let mut tree = self.tree.clone();
        let mut db = self.db.clone();
        let mut stack = Vec::new();
        if let Some(p) = &self.path {
            if let Ok(c) = p.canonicalize() {
                stack.push(c);
            }
        }
        inline_transfers(&mut tree.root, &self.dir(), &mut db, &mut stack, opts)?;
        strip_transfer_out(&mut tree.root);
        check_depth(&tree)?;
        Ok(Project { tree, db })
    }
}
