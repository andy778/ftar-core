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
        [dir.join(name), dir.join(ped_file_name(name))]
            .into_iter()
            .find(|p| self.allows(p))
    }
}

fn ped_file_name(name: &str) -> &str {
    name.rsplit(['/', '\\']).next().unwrap_or(name)
}

/// Where the database `name` of a tree saved in `dir` is written, matching
/// where it is read from: a relative name (`lift.ped`, `../library/sensors.ped`)
/// is relative to the tree, so one `.ped` can be shared by trees in several
/// folders. An absolute name is used when that file exists; otherwise (an old
/// file from another machine) the file name next to the tree.
/// After the tree's database name changed: when that file exists for a tree
/// in `dir`, read its events into `db` (they replace events with the same id)
/// and give the tree's nodes their database types, as on load. This keeps a
/// shared database's data, and stops the next save from overwriting it with
/// only this tree's events. Returns how many events were read (0 when the
/// file does not exist yet; it is created on save).
pub fn load_database_events(
    tree: &mut Tree,
    db: &mut PrimaryEventDb,
    dir: &Path,
    opts: &LoadOptions,
) -> Result<usize> {
    let Some(name) = tree.db_name.as_deref() else {
        return Ok(0);
    };
    let p = ped_save_path(dir, name);
    if !opts.allows(&p) {
        return Ok(0);
    }
    let ped = parse_ped(&opts.read(&p)?)?;
    for (k, v) in &ped.events {
        db.events.insert(k.clone(), v.clone());
    }
    apply_ped_types(&mut tree.root, &ped);
    Ok(ped.events.len())
}

/// `..` is folded into the folder it leaves (`trees/../lib/x.ped` becomes
/// `lib/x.ped`); one that climbs above `dir`'s start is kept.
pub fn ped_save_path(dir: &Path, name: &str) -> PathBuf {
    use std::path::Component;
    let p = Path::new(name);
    let joined = if p.is_relative() || p.is_file() {
        dir.join(name)
    } else {
        dir.join(ped_file_name(name))
    };
    let mut out = PathBuf::new();
    for c in joined.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir
                if matches!(out.components().next_back(), Some(Component::Normal(_))) =>
            {
                out.pop();
            }
            c => out.push(c),
        }
    }
    out
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

/// Read one tree file with its own event database: an `.fta` and the `.ped`
/// it names, or a text-format file ([`crate::text`]), which holds both.
fn read_tree_file(
    path: &Path,
    opts: &LoadOptions,
) -> Result<(Tree, Option<PrimaryEventDb>, crate::text::Extras)> {
    let bytes = opts.read(path)?;
    if crate::text::is_text_path(path) {
        let t = crate::text::parse_text(&bytes)?;
        return Ok((t.tree, Some(t.db), t.extras));
    }
    let tree = parse_fta(&bytes)?;
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let ped = match tree.db_name.as_deref().and_then(|n| opts.find_ped(dir, n)) {
        Some(p) => Some(parse_ped(&opts.read(&p)?)?),
        None => None,
    };
    Ok((tree, ped, crate::text::Extras::default()))
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

    let (mut tree, own_ped, _) = read_tree_file(path, opts)?;
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    if let Some(ped) = &own_ped {
        for (k, v) in &ped.events {
            db.events.entry(k.clone()).or_insert_with(|| v.clone());
        }
        // As in OpenFTA's `load_item`, the event type recorded in the tree's own
        // database overrides the type letter stored in the `.fta` file.
        apply_ped_types(&mut tree.root, ped);
    }

    inline_transfers(&mut tree.root, dir, db, stack, opts)?;
    stack.pop();
    Ok(tree)
}

pub(crate) fn apply_ped_types(n: &mut Node, ped: &PrimaryEventDb) {
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
    /// What a text-format file holds beyond the tree and database (models,
    /// comments, times, ...); written back when saving as text.
    pub extras: crate::text::Extras,
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
    /// Load a tree and its own `.ped` without following transfers. A
    /// text-format file (`.ftt`, see [`crate::text`]) carries its database
    /// itself.
    pub fn load(path: impl AsRef<Path>) -> Result<Document> {
        Self::load_with(path, &LoadOptions::default())
    }

    /// Like [`Document::load`], under the limits in `opts`.
    pub fn load_with(path: impl AsRef<Path>, opts: &LoadOptions) -> Result<Document> {
        let path = path.as_ref();
        let (mut tree, db, extras) = read_tree_file(path, opts)?;
        let db = db.unwrap_or_default();
        apply_ped_types(&mut tree.root, &db);
        Ok(Document {
            path: Some(path.to_path_buf()),
            tree,
            db,
            extras,
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

/// Most `.fta` files [`trees_using_event`] reads in one directory.
pub const MAX_TREES_SCANNED: usize = 10_000;

/// A tree file that uses a primary event.
#[derive(Debug, Clone, PartialEq)]
pub struct EventUse {
    pub path: PathBuf,
    /// How many of the tree's events have the id.
    pub nodes: usize,
}

/// What [`trees_using_event`] found.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EventUsage {
    /// Trees that use the event, sorted by file name.
    pub trees: Vec<EventUse>,
    /// `.fta` files that could not be read or parsed (also sorted).
    pub skipped: Vec<PathBuf>,
    /// The directory held more than [`MAX_TREES_SCANNED`] trees; the rest were not read.
    pub truncated: bool,
}

/// The database file a tree in `dir` named `name` uses, made comparable.
fn database_key(dir: &Path, name: Option<&str>) -> Option<PathBuf> {
    let p = ped_save_path(dir, name.filter(|n| !n.is_empty())?);
    Some(p.canonicalize().unwrap_or(p))
}

/// The `.fta` files in `dir` that use primary event `id` from the database
/// `database` (a tree's database name, resolved against `dir` as on load;
/// `None` for trees without one). A tree uses the event when one of its
/// primary or condition events has the id and it takes its events from the
/// same `.ped` file; an event of the same name in another database is a
/// different event. Only the files themselves are read: a tree that reaches
/// the event through a transfer-in is not listed, the transferred tree is.
/// The files are read as stored on disk, so unsaved edits are not seen.
pub fn trees_using_event(
    dir: &Path,
    id: &str,
    database: Option<&str>,
    opts: &LoadOptions,
) -> Result<EventUsage> {
    let io = |e: std::io::Error| Error::Io(format!("{}: {e}", dir.display()));
    if let Some(root) = &opts.root {
        if !dir.canonicalize().map_err(io)?.starts_with(root) {
            return Err(Error::Io(format!(
                "{}: outside the allowed directory",
                dir.display()
            )));
        }
    }
    let want = database_key(dir, database);
    let mut files = Vec::new();
    let mut usage = EventUsage::default();
    for entry in std::fs::read_dir(dir).map_err(io)? {
        let p = entry.map_err(io)?.path();
        let is_fta = p.extension().is_some_and(|e| e.eq_ignore_ascii_case("fta"));
        if is_fta && p.is_file() {
            if files.len() == MAX_TREES_SCANNED {
                usage.truncated = true;
                break;
            }
            files.push(p);
        }
    }
    files.sort();
    for p in files {
        let Ok(tree) = opts.read(&p).and_then(|b| parse_fta(&b)) else {
            usage.skipped.push(p);
            continue;
        };
        if database_key(dir, tree.db_name.as_deref()) != want {
            continue;
        }
        let nodes = tree
            .nodes()
            .iter()
            .filter(|n| {
                n.id == id
                    && (n.kind.is_primary()
                        || matches!(
                            n.kind,
                            EventType::ConditionAnalysed | EventType::ConditionNotAnalysed
                        ))
            })
            .count();
        if nodes > 0 {
            usage.trees.push(EventUse { path: p, nodes });
        }
    }
    Ok(usage)
}
