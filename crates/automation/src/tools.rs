//! Tool implementations.

use std::path::Path;

use ftar_core::analysis::{
    minimal_cut_sets_limited, probability_report, CutSet, ProbabilityOptions,
};
use ftar_core::edit::{self, History};
use ftar_core::montecarlo::{simulate, Method, MonteCarloOptions};
use ftar_core::project::{Document, LoadOptions, Project};
use ftar_core::report;
use ftar_core::write::{write_fta, write_ped};
use ftar_core::{EventType, Node, PrimaryEvent, PrimaryEventDb, Tree};
use serde_json::{json, Map, Value};

use crate::args::{kind_name, parse_kind, Args};
use crate::session::{Doc, Session, ToolError};

const MAX_TRIALS: u64 = 1_000_000_000;
const MAX_TERMS: usize = 16;
const MAX_FILE: u64 = 64 * 1024 * 1024;
const MAX_JSON_DEPTH: usize = 512;

pub fn dispatch(s: &mut Session, tool: &str, args: &Value) -> Result<Value, ToolError> {
    let a = Args(args);
    match tool {
        "tree_open" => tree_open(s, &a),
        "tree_new" => tree_new(s, &a),
        "tree_close" => tree_close(s, &a),
        "tree_list" => tree_list(s),
        "tree_info" => {
            let (id, d) = s.doc_mut(args)?;
            Ok(info(&id, d))
        }
        "tree_get" => tree_get(s, &a),
        "tree_validate" => tree_validate(s, &a),
        "tree_edit" => tree_edit(s, &a),
        "tree_undo" | "tree_redo" => tree_undo_redo(s, &a, tool == "tree_undo"),
        "tree_save" => tree_save(s, &a),
        "tree_export" => tree_export(s, &a),
        "event_list" => event_list(s, &a),
        "event_set" => event_set(s, &a),
        "event_delete" => event_delete(s, &a),
        "event_usage" => event_usage(s, &a),
        "analysis_cutsets" => analysis_cutsets(s, &a),
        "analysis_probability" => analysis_probability(s, &a),
        "analysis_montecarlo" => analysis_montecarlo(s, &a),
        "report" => make_report(s, &a, args),
        other => Err(ToolError::new(
            "unknown_tool",
            format!("no tool named `{other}`"),
        )),
    }
}

/// Close a document; refuses unsaved changes unless `discard` is true.
fn tree_close(s: &mut Session, a: &Args) -> Result<Value, ToolError> {
    let (id, d) = s.doc_mut(a.0)?;
    if d.dirty && !a.opt_bool("discard")?.unwrap_or(false) {
        return Err(ToolError::new(
            "unsaved",
            format!("`{id}` has unsaved changes: save it with tree_save, or pass `discard: true`"),
        ));
    }
    s.docs.remove(&id);
    if s.last.as_deref() == Some(&id) {
        s.last = s.docs.keys().next_back().cloned();
    }
    Ok(json!({ "closed": id }))
}

/// The open documents.
fn tree_list(s: &Session) -> Result<Value, ToolError> {
    Ok(json!({ "documents": s.docs.iter().map(|(id, d)| json!({
            "doc": id, "name": d.name, "path": d.path.as_ref().map(|p| display_path(p)), "dirty": d.dirty
        })).collect::<Vec<_>>() }))
}

/// The node at `path`, as JSON down to `depth` levels.
fn tree_get(s: &mut Session, a: &Args) -> Result<Value, ToolError> {
    let (_, d) = s.doc_mut(a.0)?;
    let path = a.opt_path("path")?.unwrap_or_default();
    let depth = a
        .opt_usize("depth")?
        .unwrap_or(MAX_JSON_DEPTH)
        .min(MAX_JSON_DEPTH);
    let n = edit::node_at(&d.tree, &path)
        .ok_or_else(|| ToolError::new("not_found", "no node at that path"))?;
    Ok(node_json(n, depth))
}

/// Validation issues, plus a warning per transfer that was not inlined.
fn tree_validate(s: &mut Session, a: &Args) -> Result<Value, ToolError> {
    let (_, d) = s.doc_mut(a.0)?;
    let issues = edit::validate(&d.tree);
    let mut list: Vec<Value> = issues
        .iter()
        .map(|i| {
            json!({
                "path": i.path,
                "severity": if i.severity == edit::Severity::Error { "error" } else { "warning" },
                "message": i.message,
            })
        })
        .collect();
    let transfers = raw_transfers(&d.tree);
    for (path, id) in &transfers {
        list.push(json!({
            "path": path,
            "severity": "warning",
            "message": format!("transfer-in `{id}` is not inlined (opened with raw=true): reopen without raw to analyse"),
        }));
    }
    Ok(json!({
        "ok": !issues.iter().any(|i| i.severity == edit::Severity::Error),
        "analysable": transfers.is_empty(),
        "issues": list,
    }))
}

/// Undo or redo one edit of the tree and its event database.
fn tree_undo_redo(s: &mut Session, a: &Args, undo: bool) -> Result<Value, ToolError> {
    let (_, d) = s.doc_mut(a.0)?;
    // history snapshots hold the tree *and* the event database
    let now = (d.tree.clone(), d.db.clone());
    let next = if undo {
        d.history.undo(&now)
    } else {
        d.history.redo(&now)
    };
    match next {
        Some((t, db)) => {
            d.tree = t;
            d.db = db;
            d.dirty = true;
            Ok(
                json!({ "applied": true, "can_undo": d.history.can_undo(), "can_redo": d.history.can_redo() }),
            )
        }
        None => Ok(
            json!({ "applied": false, "can_undo": d.history.can_undo(), "can_redo": d.history.can_redo() }),
        ),
    }
}

/// The tree's event database, sorted by id.
fn event_list(s: &mut Session, a: &Args) -> Result<Value, ToolError> {
    let (_, d) = s.doc_mut(a.0)?;
    let mut ev: Vec<_> = d.db.events.values().collect();
    ev.sort_by(|x, y| x.id.cmp(&y.id));
    Ok(json!({ "events": ev.iter().map(|e| event_json(e)).collect::<Vec<_>>() }))
}

fn analysis_cutsets(s: &mut Session, a: &Args) -> Result<Value, ToolError> {
    let (_, d) = s.doc_mut(a.0)?;
    ensure_analysable(d)?;
    let sets = minimal_cut_sets_limited(&d.tree, a.opt_usize("max_order")?.unwrap_or(usize::MAX))?;
    Ok(json!({ "count": sets.len(), "cut_sets": sets.iter().map(cs_json).collect::<Vec<_>>() }))
}

fn analysis_probability(s: &mut Session, a: &Args) -> Result<Value, ToolError> {
    let (_, d) = s.doc_mut(a.0)?;
    ensure_analysable(d)?;
    let opts = prob_opts(a)?;
    let r = probability_report(&d.tree, &d.db, &opts)?;
    Ok(prob_json(&r))
}

fn analysis_montecarlo(s: &mut Session, a: &Args) -> Result<Value, ToolError> {
    let (_, d) = s.doc_mut(a.0)?;
    ensure_analysable(d)?;
    let r = simulate(&d.tree, &d.db, &mc_opts(a)?)?;
    Ok(mc_json(&r))
}

// ------------------------------------------------------------- documents

/// Windows canonical paths carry a `\\?\` prefix that is noise in JSON.
fn display_path(p: &Path) -> String {
    let s = p.display().to_string();
    s.strip_prefix(r"\\?\").map(str::to_string).unwrap_or(s)
}

fn tree_open(s: &mut Session, a: &Args) -> Result<Value, ToolError> {
    let path = s.resolve(a.str("path")?)?;
    let raw = a.opt_bool("raw")?.unwrap_or(false);
    // The tree names further files (its database, transfer-in trees); they
    // must stay inside the root and under the size limit too.
    let opts = LoadOptions {
        root: s.root.clone(),
        max_file_size: Some(MAX_FILE),
    };
    let (tree, db) = if raw {
        let d = Document::load_with(&path, &opts)?;
        (d.tree, d.db)
    } else {
        let p = Project::load_with(&path, &opts)?;
        (p.tree, p.db)
    };
    let name = path
        .file_stem()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let id = s.alloc_id();
    s.docs.insert(
        id.clone(),
        Doc {
            name,
            path: Some(path),
            tree,
            db,
            history: History::new(),
            dirty: false,
        },
    );
    s.last = Some(id.clone());
    let d = &s.docs[&id];
    Ok(info(&id, d))
}

fn tree_new(s: &mut Session, a: &Args) -> Result<Value, ToolError> {
    let mut top = edit::new_node(EventType::Intermediate, "");
    top.children.push(edit::new_node(EventType::Or, ""));
    let tree = Tree {
        db_name: None,
        note: String::new(),
        root: top,
    };
    let id = s.alloc_id();
    let name = a.opt_str("name")?.unwrap_or("untitled").to_string();
    s.docs.insert(
        id.clone(),
        Doc {
            name,
            path: None,
            tree,
            db: PrimaryEventDb::default(),
            history: History::new(),
            // nothing to lose until the first edit
            dirty: false,
        },
    );
    s.last = Some(id.clone());
    Ok(info(&id, &s.docs[&id]))
}

fn info(id: &str, d: &Doc) -> Value {
    let nodes = d.tree.nodes();
    json!({
        "doc": id,
        "name": d.name,
        "path": d.path.as_ref().map(|p| display_path(p)),
        "database": d.tree.db_name,
        "note": d.tree.note,
        "nodes": nodes.len(),
        "gates": nodes.iter().filter(|n| n.kind.is_gate()).count(),
        "primary_events": nodes.iter().filter(|n| n.kind.is_primary()).count(),
        "database_events": d.db.events.len(),
        "dirty": d.dirty,
        "can_undo": d.history.can_undo(),
        "can_redo": d.history.can_redo(),
    })
}

fn node_json(n: &Node, depth: usize) -> Value {
    let mut m = Map::new();
    m.insert("kind".into(), json!(kind_name(n.kind)));
    m.insert("code".into(), json!(n.kind.code().to_string()));
    m.insert("id".into(), json!(n.id));
    if !n.text.is_empty() {
        m.insert("text".into(), json!(n.text));
    }
    m.insert("child_count".into(), json!(n.children.len()));
    if depth > 0 {
        m.insert(
            "children".into(),
            Value::Array(n.children.iter().map(|c| node_json(c, depth - 1)).collect()),
        );
    }
    Value::Object(m)
}

fn event_json(e: &PrimaryEvent) -> Value {
    json!({
        "id": e.id, "kind": kind_name(e.kind), "description": e.description,
        "probability": e.probability, "rate": e.lambda == Some('L'), "dormant": e.modifier == Some('D'),
        "group": e.group,
    })
}

// ------------------------------------------------------------- editing

fn tree_edit(s: &mut Session, a: &Args) -> Result<Value, ToolError> {
    let load_opts = LoadOptions {
        root: s.root.clone(),
        max_file_size: Some(MAX_FILE),
    };
    let (_, d) = s.doc_mut(a.0)?;
    let op = a.str("op")?;
    // Parse arguments first so a bad request never touches the tree or the history.
    let result: Value = match op {
        "insert_child" | "delete" | "move" | "copy" | "reorder" => edit_structure(d, a, op)?,
        "change_type" | "set_id" | "set_text" | "set_database" | "set_note" => {
            edit_properties(d, a, op, &load_opts)?
        }
        other => return Err(ToolError::args(format!("unknown op `{other}`"))),
    };
    d.dirty = true;
    Ok(result)
}

/// Edits that add, remove or rearrange nodes.
fn edit_structure(d: &mut Doc, a: &Args, op: &str) -> Result<Value, ToolError> {
    Ok(match op {
        "insert_child" => {
            let parent = a.opt_path("parent")?.unwrap_or_default();
            let kind = a.kind("kind")?;
            let mut node = edit::new_node(kind, a.opt_str("id")?.unwrap_or(""));
            if let Some(t) = a.opt_str("text")? {
                node.text = t.to_string();
            }
            let index = a.opt_usize("index")?;
            let p = d.edit_tree(|t| {
                let at = match index {
                    Some(i) => i,
                    None => edit::node_at(t, &parent)
                        .ok_or(edit::EditError::InvalidPath)?
                        .children
                        .len(),
                };
                edit::insert_child(t, &parent, at, node)
            })?;
            json!({ "path": p })
        }
        "delete" => {
            let path = a.path("path")?;
            d.edit_tree(|t| edit::delete_subtree(t, &path))?;
            json!({ "deleted": path })
        }
        "move" => {
            let (from, to, index) = (
                a.path("path")?,
                a.path("new_parent")?,
                a.opt_usize("index")?,
            );
            let p = d.edit_tree(|t| {
                // default: append; siblings are counted after the node is taken out
                let at = match index {
                    Some(i) => i,
                    None => {
                        let n = edit::node_at(t, &to)
                            .ok_or(edit::EditError::InvalidPath)?
                            .children
                            .len();
                        let same_parent = from.len() == to.len() + 1 && from.starts_with(&to);
                        n - usize::from(same_parent)
                    }
                };
                edit::move_subtree(t, &from, &to, at)
            })?;
            json!({ "path": p })
        }
        "copy" => {
            // paste a copy of the branch at `path` under `new_parent` (default: append)
            let (from, to, index) = (
                a.path("path")?,
                a.path("new_parent")?,
                a.opt_usize("index")?,
            );
            let p = d.edit_tree(|t| {
                let clip = edit::copy_subtree(t, &from)?;
                let at = match index {
                    Some(i) => i,
                    None => edit::node_at(t, &to)
                        .ok_or(edit::EditError::InvalidPath)?
                        .children
                        .len(),
                };
                edit::paste_subtree(t, &to, at, &clip)
            })?;
            json!({ "path": p })
        }
        "reorder" => {
            let (path, index) = (
                a.path("path")?,
                a.opt_usize("index")?
                    .ok_or_else(|| ToolError::args("missing `index`"))?,
            );
            let p = d.edit_tree(|t| edit::reorder(t, &path, index))?;
            json!({ "path": p })
        }
        other => return Err(ToolError::args(format!("unknown op `{other}`"))),
    })
}

/// Edits of a node's or the tree's own fields.
fn edit_properties(
    d: &mut Doc,
    a: &Args,
    op: &str,
    load_opts: &LoadOptions,
) -> Result<Value, ToolError> {
    Ok(match op {
        "change_type" => {
            let (path, kind) = (a.path("path")?, a.kind("kind")?);
            d.edit_tree(|t| edit::change_type(t, &path, kind, None))?;
            json!({ "path": path, "kind": kind_name(kind) })
        }
        "set_id" => {
            let (path, id) = (a.path("path")?, a.str("id")?);
            d.edit_tree(|t| edit::set_id(t, &path, id))?;
            json!({ "path": path, "id": id })
        }
        "set_text" => {
            let (path, text) = (a.path("path")?, a.str("text")?);
            d.edit_tree(|t| edit::set_text(t, &path, text))?;
            json!({ "path": path })
        }
        "set_database" => {
            let name = a.opt_str("name")?;
            // An existing database file is read in, so its events are used
            // and a later save does not overwrite it with only this tree's.
            let dir = d
                .path
                .as_deref()
                .and_then(Path::parent)
                .map(Path::to_path_buf);
            let loaded = d.edit_both(|t, db| -> Result<usize, ToolError> {
                edit::set_db_name(t, name)?;
                match &dir {
                    Some(dir) => Ok(ftar_core::project::load_database_events(
                        t, db, dir, load_opts,
                    )?),
                    None => Ok(0),
                }
            })?;
            json!({ "database": d.tree.db_name, "events_loaded": loaded })
        }
        "set_note" => {
            let note = a.str("note")?;
            d.edit_tree(|t| -> Result<(), edit::EditError> {
                edit::set_note(t, note);
                Ok(())
            })?;
            json!({})
        }
        other => return Err(ToolError::args(format!("unknown op `{other}`"))),
    })
}

fn event_set(s: &mut Session, a: &Args) -> Result<Value, ToolError> {
    let (_, d) = s.doc_mut(a.0)?;
    let id = a.str("id")?;
    if id.is_empty()
        || id
            .chars()
            .any(|c| c.is_whitespace() || c == ';' || c == '\0')
    {
        return Err(ToolError::args("invalid event id"));
    }
    let p = a
        .opt_f64("probability")?
        .ok_or_else(|| ToolError::args("missing `probability`"))?;
    let rate = a.opt_bool("rate")?;
    if rate != Some(true) && !(0.0..=1.0).contains(&p) {
        return Err(ToolError::args(
            "probability must be within 0..1 (use rate=true for failure rates)",
        ));
    }
    let kind = match a.opt_str("kind")? {
        Some(k) => parse_kind(k)?,
        None => {
            d.db.events
                .get(id)
                .map(|e| e.kind)
                .unwrap_or(EventType::Basic)
        }
    };
    let old = d.db.events.get(id).cloned();
    let dormant = a.opt_bool("dormant")?;
    let ev = PrimaryEvent {
        id: id.to_string(),
        group: old.as_ref().map(|e| e.group.clone()).unwrap_or_default(),
        kind,
        modifier: match dormant {
            Some(true) => Some('D'),
            Some(false) => None,
            None => old.as_ref().and_then(|e| e.modifier),
        },
        description: a
            .opt_str("description")?
            .map(str::to_string)
            .or_else(|| old.as_ref().map(|e| e.description.clone()))
            .unwrap_or_default(),
        probability: p,
        lambda: match rate {
            Some(true) => Some('L'),
            Some(false) => None,
            None => old.as_ref().and_then(|e| e.lambda),
        },
    };
    let out = event_json(&ev);
    // recorded for undo, like tree edits (a failure can't happen here, but stay uniform)
    d.edit_both(|tree, db| ftar_core::database::upsert_event(tree, db, ev))?;
    d.dirty = true;
    Ok(out)
}

fn event_delete(s: &mut Session, a: &Args) -> Result<Value, ToolError> {
    let (_, d) = s.doc_mut(a.0)?;
    let id = a.str("id")?;
    if !d.db.events.contains_key(id) {
        return Err(ToolError::args(format!("no event `{id}` in the database")));
    }
    let used = d.edit_both(|tree, db| {
        ftar_core::database::remove_event(tree, db, id)
            .ok_or_else(|| ToolError::args("no such event"))
    })?;
    d.dirty = true;
    Ok(json!({ "deleted": id, "still_used_by_nodes": used }))
}

fn event_usage(s: &mut Session, a: &Args) -> Result<Value, ToolError> {
    let opts = LoadOptions {
        root: s.root.clone(),
        max_file_size: Some(MAX_FILE),
    };
    let (_, d) = s.doc_mut(a.0)?;
    let id = a.str("id")?;
    let path = d
        .path
        .clone()
        .ok_or_else(|| ToolError::args("document has no path; save it first"))?;
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let here = d
        .tree
        .nodes()
        .iter()
        .filter(|n| n.id == id && uses_event(n.kind))
        .count();
    let u = ftar_core::project::trees_using_event(dir, id, d.tree.db_name.as_deref(), &opts)?;
    let me = path.canonicalize().ok();
    let others: Vec<Value> = u
        .trees
        .iter()
        .filter(|t| t.path.canonicalize().ok() != me)
        .map(|t| json!({ "path": display_path(&t.path), "nodes": t.nodes }))
        .collect();
    Ok(json!({
        "id": id,
        "database": d.tree.db_name,
        "in_database": d.db.events.contains_key(id),
        "this_tree_nodes": here,
        "other_trees": others,
        "skipped": u.skipped.iter().map(|p| display_path(p)).collect::<Vec<_>>(),
        "truncated": u.truncated,
    }))
}

fn uses_event(k: EventType) -> bool {
    k.is_primary()
        || matches!(
            k,
            EventType::ConditionAnalysed | EventType::ConditionNotAnalysed
        )
}

fn tree_save(s: &mut Session, a: &Args) -> Result<Value, ToolError> {
    let target = match a.opt_str("path")? {
        Some(p) => Some(s.resolve(p)?),
        None => None,
    };
    let ped_arg = a.opt_bool("ped")?;
    let (_, d) = s.doc_mut(a.0)?;
    let path = target
        .or_else(|| d.path.clone())
        .ok_or_else(|| ToolError::args("document has no path; pass `path`"))?;
    // By default the database is written whenever there is one to write, so
    // event probabilities are never silently left unsaved.
    let has_events = !d.db.events.is_empty();
    let write_db = ped_arg.unwrap_or(has_events || d.tree.db_name.is_some());
    // As in the GUI, a tree without a database name gets `<stem>.ped`.
    let db_name = match (&d.tree.db_name, write_db) {
        (Some(n), _) => Some(n.clone()),
        (None, true) => Some(format!(
            "{}.ped",
            path.file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "tree".into())
        )),
        (None, false) => None,
    };
    // The .ped goes where it is read from (relative to the tree), checked like
    // any other path, so it cannot escape the server's root.
    let ped_path = match (&db_name, write_db) {
        (Some(name), true) => {
            let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
            if matches!(base, "" | "." | "..") {
                return Err(ToolError::args(format!(
                    "database name `{name}` is not a file name"
                )));
            }
            let p = ftar_core::project::ped_save_path(
                path.parent().unwrap_or_else(|| Path::new(".")),
                name,
            );
            Some(s.resolve(&p.to_string_lossy())?)
        }
        _ => None,
    };

    let (_, d) = s.doc_mut(a.0)?;
    d.tree.db_name = db_name;
    std::fs::write(&path, write_fta(&d.tree))
        .map_err(|e| ToolError::new("io", format!("{}: {e}", display_path(&path))))?;
    let mut written = vec![display_path(&path)];
    if let Some(ped) = &ped_path {
        std::fs::write(ped, write_ped(&d.db))
            .map_err(|e| ToolError::new("io", format!("{}: {e}", display_path(ped))))?;
        written.push(display_path(ped));
    }
    d.path = Some(path);
    d.dirty = false;
    let mut out = json!({ "written": written });
    if has_events && ped_path.is_none() {
        out["warning"] = json!("the event database was not written (`ped: false`)");
    }
    Ok(out)
}

// ------------------------------------------------------------- analysis

fn cs_json(cs: &CutSet) -> Value {
    json!(cs.iter().collect::<Vec<_>>())
}

fn prob_opts(a: &Args) -> Result<ProbabilityOptions, ToolError> {
    let d = ProbabilityOptions::default();
    let terms = a.opt_usize("terms")?.unwrap_or(d.terms);
    if terms == 0 || terms > MAX_TERMS {
        return Err(ToolError::args(format!("`terms` must be 1..={MAX_TERMS}")));
    }
    Ok(ProbabilityOptions {
        max_order: a.opt_usize("max_order")?.unwrap_or(d.max_order),
        terms,
        unit_time: a.opt_f64("unit_time")?.unwrap_or(d.unit_time),
    })
}

fn mc_opts(a: &Args) -> Result<MonteCarloOptions, ToolError> {
    let d = MonteCarloOptions::default();
    let trials = a.opt_u64("trials")?.unwrap_or(d.trials);
    if trials == 0 || trials > MAX_TRIALS {
        return Err(ToolError::args(format!(
            "`trials` must be 1..={MAX_TRIALS}"
        )));
    }
    Ok(MonteCarloOptions {
        trials,
        unit_time: a.opt_f64("unit_time")?.unwrap_or(d.unit_time),
        seed: a.opt_u64("seed")?.unwrap_or(d.seed),
        method: match a.opt_str("method")? {
            None | Some("first_failure") => Method::FirstFailure,
            Some("direct") => Method::Direct,
            Some(m) => {
                return Err(ToolError::args(format!(
                    "unknown method `{m}` (first_failure, direct)"
                )))
            }
        },
    })
}

fn prob_json(r: &ftar_core::analysis::ProbabilityReport) -> Value {
    json!({
        "top": r.top,
        "exact": r.exact,
        "terms": r.terms,
        "partial_sums": r.partial_sums,
        "cut_sets": r.cut_sets.iter().map(|(c, p)| json!({ "events": cs_json(c), "probability": p })).collect::<Vec<_>>(),
        "events": r.events.iter().map(|e| json!({
            "id": e.id, "contribution": e.contribution, "importance_pct": e.importance_pct
        })).collect::<Vec<_>>(),
    })
}

fn mc_json(r: &ftar_core::montecarlo::MonteCarloReport) -> Value {
    let bound = r.upper_bound_95();
    json!({
        // With no failures there is no standard error, only an upper bound.
        "std_error": if bound.is_some() { Value::Null } else { json!(r.std_error) },
        "upper_bound_95": bound,
        "method": if r.method == Method::Direct { "direct" } else { "first_failure" },
        "trials": r.trials,
        "system_failures": r.system_failures,
        "bas_prob": r.bas_prob,
        "probability": r.probability,
        "any_component_failure": r.any_component_failure,
        "cut_set_hits": r.cut_set_hits.iter().map(|(c, n)| json!({ "events": cs_json(c), "hits": n })).collect::<Vec<_>>(),
        "modes": r.modes.iter().map(|(c, n)| json!({ "events": cs_json(c), "count": n })).collect::<Vec<_>>(),
        "event_failures": r.event_failures.iter().map(|(e, n)| json!({ "id": e, "failures": n })).collect::<Vec<_>>(),
    })
}

/// Transfer-in nodes left in a tree opened with `raw: true`, with their paths.
fn raw_transfers(tree: &Tree) -> Vec<(Vec<usize>, String)> {
    edit::all_paths(tree)
        .into_iter()
        .filter_map(|p| {
            let n = edit::node_at(tree, &p)?;
            (n.kind == EventType::TransferIn).then(|| (p, n.id.clone()))
        })
        .collect()
}

fn ensure_analysable(d: &Doc) -> Result<(), ToolError> {
    match raw_transfers(&d.tree).first() {
        None => Ok(()),
        Some((_, id)) => Err(ToolError::new(
            "raw_transfer",
            format!(
                "transfer-in `{id}` is not inlined because the tree was opened with raw=true; \
                 reopen it without raw to analyse it"
            ),
        )),
    }
}

fn make_report(s: &mut Session, a: &Args, args: &Value) -> Result<Value, ToolError> {
    let (_, d) = s.doc_mut(args)?;
    ensure_analysable(d)?;
    let name = a.str("kind")?;
    let kind = report::ReportKind::from_slug(name)
        .or(if name == "montecarlo" {
            Some(report::ReportKind::MonteCarlo)
        } else {
            None
        })
        .ok_or_else(|| ToolError::args(format!("unknown report kind `{name}`")))?;
    let params = report::ReportParams {
        max_order: a.opt_usize("max_order")?.unwrap_or(usize::MAX),
        terms: prob_opts(a)?.terms,
        monte_carlo: mc_opts(a)?,
    };
    let text = report::generate(
        kind,
        &d.tree,
        &d.db,
        // OpenFTA prints the file name, extension included.
        &d.path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| d.name.clone()),
        a.opt_str("time")?.unwrap_or(""),
        &params,
    )?;
    Ok(json!({ "kind": name, "text": text }))
}

fn tree_export(s: &mut Session, a: &Args) -> Result<Value, ToolError> {
    let path = s.resolve(a.str("path")?)?;
    let (_, d) = s.doc_mut(a.0)?;
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let format = a
        .opt_str("format")?
        .map(str::to_ascii_lowercase)
        .unwrap_or(ext);
    let title = a
        .opt_str("title")?
        .map(str::to_string)
        .unwrap_or_else(|| d.name.clone());
    let probabilities = a.opt_bool("probabilities")?.unwrap_or(true);
    let setup = ftar_render::PageSetup::from_options(
        a.opt_str("page_size")?,
        a.opt_str("orientation")?,
        a.opt_str("fit")?,
        a.opt_f64("scale_percent")?,
        a.opt_f64("margin_mm")?,
    )
    .map_err(ToolError::args)?;
    let sheet = a.opt_f64("sheet")?;
    let bytes: Vec<u8> = match format.as_str() {
        "svg" | "png" => {
            let svg = if let Some(n) = sheet {
                // one printed sheet of the page layout (print preview)
                let pages = ftar_render::print_pages(&d.tree, &d.db, &title, probabilities, &setup);
                let idx = (n as usize).checked_sub(1).filter(|_| n >= 1.0);
                idx.and_then(|i| pages.get(i).cloned()).ok_or_else(|| {
                    ToolError::args(format!("`sheet` must be 1..={} (got {n})", pages.len()))
                })?
            } else {
                ftar_render::tree_svg(
                    &d.tree,
                    &d.db,
                    &ftar_render::SvgOptions {
                        title,
                        probabilities,
                        background: true,
                    },
                )
            };
            if format == "svg" {
                svg.into_bytes()
            } else {
                let scale = a.opt_f64("scale")?.unwrap_or(2.0) as f32;
                ftar_render::svg_to_png(&svg, scale).map_err(|e| ToolError::args(e.to_string()))?
            }
        }
        "html" => ftar_render::print_html(
            &d.tree,
            &d.db,
            &ftar_render::PrintOptions {
                title,
                probabilities,
                report: None,
                auto_print: false,
                page: setup,
            },
        )
        .into_bytes(),
        other => {
            return Err(ToolError::args(format!(
                "unknown format `{other}` (png, svg, html; or use a matching file extension)"
            )))
        }
    };
    std::fs::write(&path, &bytes)
        .map_err(|e| ToolError::new("io", format!("{}: {e}", path.display())))?;
    let sheets = ftar_render::page_count(&d.tree, &d.db, probabilities, &setup);
    Ok(
        json!({ "written": display_path(&path), "format": format, "bytes": bytes.len(), "sheets": sheets }),
    )
}
