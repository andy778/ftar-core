//! Pure tree-editing functions for a GUI, plus an undo/redo [`History`].
//!
//! Nodes are addressed by a *path*: the list of child indices leading from
//! the root (`[]` is the root, `[0, 2]` the third child of the first child).
//!
//! The structural rules follow the original editor (`tree_add` in
//! `treeUtil.c`):
//!
//! * only gates and intermediate events can have children;
//! * a gate cannot be the child of a gate, and a transfer-out cannot be
//!   below a gate;
//! * an intermediate event has at most one real child; the root intermediate
//!   event may additionally have a transfer-out as its *first* child;
//! * a condition can only be the child of a gate, and a gate has at most one;
//! * an inhibit gate has at most two children: one event and one condition.
//!
//! Minimum child counts (a gate needs two children, an intermediate event
//! one, ...) are *not* enforced while editing, since a tree is built up step
//! by step; use [`validate`] to list what is still incomplete.
//!
//! Every function either succeeds or leaves the tree untouched.

use std::fmt;

use crate::model::{EventType, Node, Tree, MAX_DEPTH};

/// Index path from the root to a node.
pub type Path = Vec<usize>;

/// Why an edit was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditError {
    InvalidPath,
    IndexOutOfRange,
    CannotDeleteRoot,
    CannotMoveRoot,
    MoveIntoOwnSubtree,
    /// The parent is a leaf event.
    ChildrenNotAllowed(EventType),
    GateUnderGate,
    /// Transfer-out is only allowed as first child of the root intermediate event.
    TransferOutPlacement,
    /// Conditions are only allowed below gates, one per gate.
    ConditionNotAllowed,
    /// The parent cannot take another child.
    TooManyChildren,
    /// Text is only stored for intermediate events.
    NotIntermediate,
    /// Id or text contains characters the file format cannot hold.
    InvalidText(&'static str),
    /// The edit would nest the tree deeper than [`MAX_DEPTH`].
    TooDeep,
}

impl fmt::Display for EditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPath => write!(f, "no such node"),
            Self::IndexOutOfRange => write!(f, "child index out of range"),
            Self::CannotDeleteRoot => write!(f, "the top event cannot be deleted"),
            Self::CannotMoveRoot => write!(f, "the top event cannot be moved"),
            Self::MoveIntoOwnSubtree => write!(f, "cannot move a node into its own subtree"),
            Self::ChildrenNotAllowed(k) => write!(f, "`{}` events cannot have children", k.code()),
            Self::GateUnderGate => write!(f, "a gate cannot be connected directly to a gate"),
            Self::TransferOutPlacement => {
                write!(f, "a transfer-out is only allowed below the top event")
            }
            Self::ConditionNotAllowed => write!(f, "condition not allowed here"),
            Self::TooManyChildren => write!(f, "the parent cannot have more children"),
            Self::NotIntermediate => write!(f, "only intermediate events have text"),
            Self::InvalidText(m) => write!(f, "{m}"),
            Self::TooDeep => write!(
                f,
                "the tree would be nested more than {MAX_DEPTH} levels deep"
            ),
        }
    }
}

impl std::error::Error for EditError {}

pub type EditResult<T> = std::result::Result<T, EditError>;

fn is_condition(k: EventType) -> bool {
    matches!(
        k,
        EventType::ConditionNotAnalysed | EventType::ConditionAnalysed
    )
}

/// Borrow the node at `path`.
pub fn node_at<'a>(tree: &'a Tree, path: &[usize]) -> Option<&'a Node> {
    let mut n = &tree.root;
    for &i in path {
        n = n.children.get(i)?;
    }
    Some(n)
}

/// Mutably borrow the node at `path`. Prefer the checked edit functions;
/// direct mutation bypasses the structural rules.
pub fn node_at_mut<'a>(tree: &'a mut Tree, path: &[usize]) -> Option<&'a mut Node> {
    let mut n = &mut tree.root;
    for &i in path {
        n = n.children.get_mut(i)?;
    }
    Some(n)
}

/// Paths of all nodes in pre-order (the order of the file).
pub fn all_paths(tree: &Tree) -> Vec<Path> {
    fn rec(n: &Node, cur: &mut Path, out: &mut Vec<Path>) {
        out.push(cur.clone());
        for (i, c) in n.children.iter().enumerate() {
            cur.push(i);
            rec(c, cur, out);
            cur.pop();
        }
    }
    let mut out = Vec::new();
    rec(&tree.root, &mut Vec::new(), &mut out);
    out
}

/// Create a fresh childless node.
pub fn new_node(kind: EventType, id: &str) -> Node {
    Node {
        kind,
        modifier: None,
        id: id.to_string(),
        text: String::new(),
        children: Vec::new(),
    }
}

/// May `child` be placed at `index` among `existing` (the siblings, without
/// the child itself) below `parent`?
fn check_child(
    parent: &Node,
    parent_is_root: bool,
    existing: &[Node],
    index: usize,
    child: &Node,
) -> EditResult<()> {
    if index > existing.len() {
        return Err(EditError::IndexOutOfRange);
    }
    let pk = parent.kind;
    if !pk.is_gate() && pk != EventType::Intermediate {
        return Err(EditError::ChildrenNotAllowed(pk));
    }
    let ck = child.kind;
    if ck == EventType::TransferOut {
        if pk != EventType::Intermediate
            || !parent_is_root
            || index != 0
            || existing
                .first()
                .is_some_and(|c| c.kind == EventType::TransferOut)
        {
            return Err(EditError::TransferOutPlacement);
        }
        return Ok(());
    }
    if existing
        .first()
        .is_some_and(|c| c.kind == EventType::TransferOut)
        && index == 0
    {
        // Nothing may be placed before the transfer-out.
        return Err(EditError::TransferOutPlacement);
    }
    if pk == EventType::Intermediate {
        if is_condition(ck) {
            return Err(EditError::ConditionNotAllowed);
        }
        if existing.iter().any(|c| c.kind != EventType::TransferOut) {
            return Err(EditError::TooManyChildren);
        }
        return Ok(());
    }
    // gate
    if ck.is_gate() {
        return Err(EditError::GateUnderGate);
    }
    let has_cond = existing.iter().any(|c| is_condition(c.kind));
    if is_condition(ck) && has_cond {
        return Err(EditError::ConditionNotAllowed);
    }
    if pk == EventType::Inhibit {
        if existing.len() >= 2 {
            return Err(EditError::TooManyChildren);
        }
        if !is_condition(ck) && existing.iter().any(|c| !is_condition(c.kind)) {
            return Err(EditError::TooManyChildren);
        }
    }
    Ok(())
}

/// Check that a whole subtree obeys the structural rules.
fn check_subtree(n: &Node, is_root: bool) -> EditResult<()> {
    for (i, c) in n.children.iter().enumerate() {
        check_child(n, is_root, &n.children[..i], i, c)?;
        check_subtree(c, false)?;
    }
    Ok(())
}

fn split_last(path: &[usize]) -> EditResult<(&[usize], usize)> {
    match path.split_last() {
        Some((&i, parent)) => Ok((parent, i)),
        None => Err(EditError::InvalidPath),
    }
}

/// Insert `node` (with its subtree) as child `index` of the node at
/// `parent`. Returns the path of the inserted node.
pub fn insert_child(
    tree: &mut Tree,
    parent: &[usize],
    index: usize,
    node: Node,
) -> EditResult<Path> {
    if parent.len() + 1 + node.depth() > MAX_DEPTH {
        return Err(EditError::TooDeep);
    }
    check_subtree(&node, false)?;
    let p = node_at(tree, parent).ok_or(EditError::InvalidPath)?;
    check_child(p, parent.is_empty(), &p.children, index, &node)?;
    node_at_mut(tree, parent)
        .unwrap()
        .children
        .insert(index, node);
    let mut path = parent.to_vec();
    path.push(index);
    Ok(path)
}

/// Append `node` as the last child of `parent` (the TransferOut position is
/// respected by [`insert_child`]; use that for transfer-outs).
pub fn append_child(tree: &mut Tree, parent: &[usize], node: Node) -> EditResult<Path> {
    let n = node_at(tree, parent)
        .ok_or(EditError::InvalidPath)?
        .children
        .len();
    insert_child(tree, parent, n, node)
}

/// Remove and return the subtree at `path`.
pub fn delete_subtree(tree: &mut Tree, path: &[usize]) -> EditResult<Node> {
    if path.is_empty() {
        return Err(EditError::CannotDeleteRoot);
    }
    let (parent, i) = split_last(path)?;
    node_at(tree, path).ok_or(EditError::InvalidPath)?;
    Ok(node_at_mut(tree, parent).unwrap().children.remove(i))
}

/// Clone the subtree at `path` (for a clipboard).
pub fn copy_subtree(tree: &Tree, path: &[usize]) -> EditResult<Node> {
    node_at(tree, path).cloned().ok_or(EditError::InvalidPath)
}

/// Paste a previously copied subtree as child `index` of `parent`.
pub fn paste_subtree(
    tree: &mut Tree,
    parent: &[usize],
    index: usize,
    clip: &Node,
) -> EditResult<Path> {
    insert_child(tree, parent, index, clip.clone())
}

/// Move the subtree at `from` to become child `index` of `new_parent`.
/// Both paths and `index` refer to the tree *before* the move, except that
/// `index` counts siblings after the moved node has been taken out.
/// Returns the new path of the moved node.
pub fn move_subtree(
    tree: &mut Tree,
    from: &[usize],
    new_parent: &[usize],
    index: usize,
) -> EditResult<Path> {
    if from.is_empty() {
        return Err(EditError::CannotMoveRoot);
    }
    if new_parent.starts_with(from) {
        return Err(EditError::MoveIntoOwnSubtree);
    }
    node_at(tree, from).ok_or(EditError::InvalidPath)?;
    node_at(tree, new_parent).ok_or(EditError::InvalidPath)?;
    let backup = tree.clone();
    let node = delete_subtree(tree, from)?;
    // Removing a sibling before an ancestor of the target shifts its path.
    let mut target = new_parent.to_vec();
    let depth = from.len() - 1;
    if target.len() > depth && target[..depth] == from[..depth] && target[depth] > from[depth] {
        target[depth] -= 1;
    }
    match insert_child(tree, &target, index, node) {
        Ok(p) => Ok(p),
        Err(e) => {
            *tree = backup;
            Err(e)
        }
    }
}

/// Move the node at `path` to position `new_index` among its siblings.
/// Returns its new path.
pub fn reorder(tree: &mut Tree, path: &[usize], new_index: usize) -> EditResult<Path> {
    let (parent, _) = split_last(path).map_err(|_| EditError::CannotMoveRoot)?;
    move_subtree(tree, path, parent, new_index)
}

/// Change the type (and dormant modifier) of the node at `path`. Fails if
/// the node's children or its place under its parent would break the rules.
/// The modifier is only kept for primary event kinds.
pub fn change_type(
    tree: &mut Tree,
    path: &[usize],
    kind: EventType,
    modifier: Option<char>,
) -> EditResult<()> {
    let old = node_at(tree, path).ok_or(EditError::InvalidPath)?;
    let mut new = old.clone();
    new.kind = kind;
    new.modifier = if kind.is_primary() { modifier } else { None };
    if kind != EventType::Intermediate {
        new.text.clear();
    }
    check_subtree(&new, path.is_empty())?;
    if let Some((parent, i)) = path.split_last().map(|(i, p)| (p, *i)) {
        let p = node_at(tree, parent).unwrap();
        let mut sibs = p.children.clone();
        sibs.remove(i);
        check_child(p, parent.is_empty(), &sibs, i, &new)?;
    }
    *node_at_mut(tree, path).unwrap() = new;
    Ok(())
}

/// Set the id of the node at `path` (empty means unnamed). Ids may not
/// contain whitespace or `;`.
pub fn set_id(tree: &mut Tree, path: &[usize], id: &str) -> EditResult<()> {
    if id
        .chars()
        .any(|c| c.is_whitespace() || c == ';' || c == '\0')
    {
        return Err(EditError::InvalidText("an id cannot contain spaces or `;`"));
    }
    node_at_mut(tree, path).ok_or(EditError::InvalidPath)?.id = id.to_string();
    Ok(())
}

/// Set the description of an intermediate event (single line).
pub fn set_text(tree: &mut Tree, path: &[usize], text: &str) -> EditResult<()> {
    if text.contains(['\n', '\r', '\0']) {
        return Err(EditError::InvalidText(
            "a description must be a single line",
        ));
    }
    let n = node_at_mut(tree, path).ok_or(EditError::InvalidPath)?;
    if n.kind != EventType::Intermediate {
        return Err(EditError::NotIntermediate);
    }
    n.text = text.to_string();
    Ok(())
}

/// Set the primary event database a tree refers to. `None` or an empty name
/// detaches it. The name is a plain file name (no directories), as the
/// loader looks the file up next to the tree.
pub fn set_db_name(tree: &mut Tree, name: Option<&str>) -> EditResult<()> {
    let name = name.map(str::trim).filter(|n| !n.is_empty());
    if let Some(n) = name {
        let backslash = char::from(92u8);
        if n.chars()
            .any(|c| c == '/' || c == backslash || c.is_control())
            || n == "."
            || n == ".."
        {
            return Err(EditError::InvalidText(
                "a database name is a plain file name, without directories",
            ));
        }
    }
    tree.db_name = name.map(str::to_string);
    Ok(())
}

/// Set the free-text note of the tree.
pub fn set_note(tree: &mut Tree, note: &str) {
    tree.note = note.to_string();
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

/// A problem found by [`validate`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    pub path: Path,
    pub severity: Severity,
    pub message: String,
}

/// Check a tree the way the original `validate_tree` does: structural
/// completeness (child counts) plus missing ids. Returns nothing for a
/// tree that is ready for analysis.
pub fn validate(tree: &Tree) -> Vec<Issue> {
    let mut out = Vec::new();
    for path in all_paths(tree) {
        let n = node_at(tree, &path).unwrap();
        let mut push = |severity, message: &str| {
            out.push(Issue {
                path: path.clone(),
                severity,
                message: message.to_string(),
            })
        };
        if n.id.is_empty() && n.kind != EventType::Intermediate && !n.kind.is_gate() {
            push(Severity::Warning, "event has no id");
        }
        match n.kind {
            EventType::Inhibit => {
                if n.children.len() != 2 || !n.children.iter().any(|c| is_condition(c.kind)) {
                    push(
                        Severity::Error,
                        "inhibit gate needs one event and one condition",
                    );
                }
            }
            k if k.is_gate() => {
                if n.children.len() < 2 {
                    push(Severity::Error, "gate needs at least two children");
                }
            }
            EventType::Intermediate
                if !n.children.iter().any(|c| c.kind != EventType::TransferOut) =>
            {
                push(Severity::Error, "intermediate event has no child");
            }
            _ => {}
        }
        if let Err(e) = check_subtree_shallow(n, path.is_empty()) {
            push(Severity::Error, &e.to_string());
        }
    }
    out
}

fn check_subtree_shallow(n: &Node, is_root: bool) -> EditResult<()> {
    if n.children.is_empty() {
        return Ok(());
    }
    for (i, c) in n.children.iter().enumerate() {
        check_child(n, is_root, &n.children[..i], i, c)?;
    }
    Ok(())
}

/// Undo/redo stack of full snapshots. Works with any `Clone` state, e.g.
/// `History<Tree>`.
#[derive(Debug, Clone)]
pub struct History<T> {
    undo: Vec<T>,
    redo: Vec<T>,
    limit: usize,
}

impl<T: Clone> Default for History<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Clone> History<T> {
    /// Unlimited-depth history (practically: 1000 steps).
    pub fn new() -> Self {
        Self::with_limit(1000)
    }

    /// Keep at most `limit` undo steps.
    pub fn with_limit(limit: usize) -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            limit: limit.max(1),
        }
    }

    /// Record `before` as the state to return to, and clear the redo stack.
    pub fn record(&mut self, before: T) {
        self.undo.push(before);
        if self.undo.len() > self.limit {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    /// Run `f` on `current`. If it succeeds the previous state is recorded
    /// for undo; if it fails `current` is restored and nothing is recorded.
    pub fn edit<R, E>(
        &mut self,
        current: &mut T,
        f: impl FnOnce(&mut T) -> Result<R, E>,
    ) -> Result<R, E> {
        let before = current.clone();
        match f(current) {
            Ok(r) => {
                self.record(before);
                Ok(r)
            }
            Err(e) => {
                *current = before;
                Err(e)
            }
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Step back: stores `current` for redo and returns the previous state.
    pub fn undo(&mut self, current: &T) -> Option<T> {
        let prev = self.undo.pop()?;
        self.redo.push(current.clone());
        Some(prev)
    }

    /// Step forward again after an undo.
    pub fn redo(&mut self, current: &T) -> Option<T> {
        let next = self.redo.pop()?;
        self.undo.push(current.clone());
        Some(next)
    }

    /// Forget all history (e.g. after loading a file).
    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }
}
