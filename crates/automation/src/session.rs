use std::collections::BTreeMap;
use std::fmt;
use std::path::{Component, Path, PathBuf};

use ftar_core::edit::History;
use ftar_core::{PrimaryEventDb, Tree};
use serde_json::{json, Value};

/// Structured tool failure.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolError {
    /// `invalid_args`, `unknown_tool`, `not_found`, `edit_rejected`, `analysis`, `io`, `confinement`.
    pub code: &'static str,
    pub message: String,
}

impl ToolError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
    pub fn args(message: impl Into<String>) -> Self {
        Self::new("invalid_args", message)
    }
    pub fn to_json(&self) -> Value {
        json!({ "code": self.code, "message": self.message })
    }
}

impl fmt::Display for ToolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl std::error::Error for ToolError {}

impl From<ftar_core::Error> for ToolError {
    fn from(e: ftar_core::Error) -> Self {
        let code = match e {
            ftar_core::Error::Io(_) => "io",
            _ => "analysis",
        };
        // Windows canonical paths carry a `\\?\` prefix that is noise in messages.
        Self::new(code, e.to_string().replace(r"\\?\", ""))
    }
}
impl From<ftar_core::edit::EditError> for ToolError {
    fn from(e: ftar_core::edit::EditError) -> Self {
        Self::new("edit_rejected", e.to_string())
    }
}

pub(crate) struct Doc {
    pub name: String,
    pub path: Option<PathBuf>,
    pub tree: Tree,
    pub db: PrimaryEventDb,
    /// Undo history; each snapshot holds the tree and the event database.
    pub history: History<(Tree, PrimaryEventDb)>,
    pub dirty: bool,
    /// What a text-format file holds beyond tree and database (models,
    /// times, comments); written back by `tree_save` to a `.ftt`.
    pub extras: ftar_core::text::Extras,
}

impl Doc {
    /// Run an edit on the tree. On success the previous state (tree and
    /// database) is recorded for undo; on failure everything is restored.
    pub(crate) fn edit_tree<R, E>(
        &mut self,
        f: impl FnOnce(&mut Tree) -> Result<R, E>,
    ) -> Result<R, E> {
        self.edit_both(|t, _| f(t))
    }

    /// Like [`Doc::edit_tree`], for edits that also touch the event database.
    pub(crate) fn edit_both<R, E>(
        &mut self,
        f: impl FnOnce(&mut Tree, &mut PrimaryEventDb) -> Result<R, E>,
    ) -> Result<R, E> {
        let before = (self.tree.clone(), self.db.clone());
        match f(&mut self.tree, &mut self.db) {
            Ok(r) => {
                self.history.record(before);
                Ok(r)
            }
            Err(e) => {
                (self.tree, self.db) = before;
                Err(e)
            }
        }
    }
}

/// Open documents plus the file-system sandbox. One per script/agent.
pub struct Session {
    pub(crate) root: Option<PathBuf>,
    pub(crate) docs: BTreeMap<String, Doc>,
    pub(crate) last: Option<String>,
    next: u32,
}

/// Does `p` start with `root`, comparing the text of the paths (no file
/// system access; the `\\?\` prefix of canonical Windows paths is ignored,
/// and case too on Windows)?
fn lexically_inside(p: &Path, root: &Path) -> bool {
    let norm = |p: &Path| {
        let s = p.to_string_lossy().replace('/', "\\");
        let s = s.strip_prefix(r"\\?\").unwrap_or(&s).to_string();
        if cfg!(windows) {
            s.to_lowercase()
        } else {
            s.replace('\\', "/")
        }
    };
    Path::new(&norm(p)).starts_with(norm(root))
}

impl Default for Session {
    fn default() -> Self {
        Self {
            root: None,
            docs: BTreeMap::new(),
            last: None,
            next: 1,
        }
    }
}

impl Session {
    pub fn new() -> Self {
        Self::default()
    }

    /// Confine every file path a tool reads or writes to `root`.
    pub fn with_root(root: impl AsRef<Path>) -> Result<Self, ToolError> {
        let root = root
            .as_ref()
            .canonicalize()
            .map_err(|e| ToolError::new("io", format!("root: {e}")))?;
        Ok(Self {
            root: Some(root),
            ..Self::default()
        })
    }

    /// Call a tool by name with a JSON argument object.
    pub fn call(&mut self, tool: &str, args: &Value) -> Result<Value, ToolError> {
        let args = if args.is_null() { &Value::Null } else { args };
        if !args.is_object() && !args.is_null() {
            return Err(ToolError::args("arguments must be a JSON object"));
        }
        crate::tools::dispatch(self, tool, args)
    }

    pub(crate) fn alloc_id(&mut self) -> String {
        let id = format!("doc{}", self.next);
        self.next += 1;
        id
    }

    pub(crate) fn doc_mut(&mut self, args: &Value) -> Result<(String, &mut Doc), ToolError> {
        let id = match args.get("doc").and_then(Value::as_str) {
            Some(s) => s.to_string(),
            None => self.last.clone().ok_or_else(|| {
                ToolError::args("no document open (pass `doc` or open one first)")
            })?,
        };
        match self.docs.get_mut(&id) {
            Some(d) => Ok((id, d)),
            None => Err(ToolError::new(
                "not_found",
                format!("no such document `{id}`"),
            )),
        }
    }

    /// Resolve a user path inside the sandbox. Existing files are
    /// canonicalised; for new files the parent must exist and be inside.
    pub(crate) fn resolve(&self, p: &str) -> Result<PathBuf, ToolError> {
        let Some(root) = &self.root else {
            return Ok(PathBuf::from(p));
        };
        let raw = Path::new(p);
        if raw.components().any(|c| matches!(c, Component::ParentDir)) {
            return Err(ToolError::new(
                "confinement",
                format!("`{p}` contains `..`"),
            ));
        }
        let full = if raw.is_absolute() {
            raw.to_path_buf()
        } else {
            root.join(raw)
        };
        let checked = match full.canonicalize() {
            Ok(c) => c,
            Err(_) => {
                let parent = full
                    .parent()
                    .and_then(|d| d.canonicalize().ok())
                    .ok_or_else(|| {
                        // A path that names a place outside the root is a
                        // confinement error, whether or not that place exists.
                        if raw.has_root() && !lexically_inside(&full, root) {
                            ToolError::new("confinement", format!("`{p}` is outside the root"))
                        } else {
                            ToolError::new("io", format!("`{p}`: parent directory not found"))
                        }
                    })?;
                parent.join(full.file_name().unwrap_or_default())
            }
        };
        if checked.starts_with(root) {
            Ok(checked)
        } else {
            Err(ToolError::new(
                "confinement",
                format!("`{p}` is outside the root"),
            ))
        }
    }
}
