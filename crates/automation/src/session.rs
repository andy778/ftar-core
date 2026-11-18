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
        Self::new(code, e.to_string())
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
    pub history: History<Tree>,
    pub dirty: bool,
}

/// Open documents plus the file-system sandbox. One per script/agent.
pub struct Session {
    root: Option<PathBuf>,
    pub(crate) docs: BTreeMap<String, Doc>,
    pub(crate) last: Option<String>,
    next: u32,
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
                        ToolError::new("io", format!("`{p}`: parent directory not found"))
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
