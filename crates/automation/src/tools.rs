//! Tool implementations.

use std::path::Path;

use ftar_core::analysis::{
    minimal_cut_sets_limited, probability_report, CutSet, ProbabilityOptions,
};
use ftar_core::edit::{self, History};
use ftar_core::montecarlo::{simulate, MonteCarloOptions};
use ftar_core::parse::{parse_fta, parse_ped};
use ftar_core::project::Project;
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
        "tree_close" => {
            let (id, _) = s.doc_mut(args)?;
            s.docs.remove(&id);
            if s.last.as_deref() == Some(&id) {
                s.last = s.docs.keys().next_back().cloned();
            }
            Ok(json!({ "closed": id }))
        }
        "tree_list" => Ok(json!({ "documents": s.docs.iter().map(|(id, d)| json!({
            "doc": id, "name": d.name, "path": d.path.as_ref().map(|p| display_path(p)), "dirty": d.dirty
        })).collect::<Vec<_>>() })),
        "tree_info" => {
            let (id, d) = s.doc_mut(args)?;
            Ok(info(&id, d))
        }
        "tree_get" => {
            let (_, d) = s.doc_mut(args)?;
            let path = a.opt_path("path")?.unwrap_or_default();
            let depth = a
                .opt_usize("depth")?
                .unwrap_or(MAX_JSON_DEPTH)
                .min(MAX_JSON_DEPTH);
            let n = edit::node_at(&d.tree, &path)
                .ok_or_else(|| ToolError::new("not_found", "no node at that path"))?;
            Ok(node_json(n, depth))
        }
        "tree_validate" => {
            let (_, d) = s.doc_mut(args)?;
            let issues = edit::validate(&d.tree);
            Ok(json!({
                "ok": !issues.iter().any(|i| i.severity == edit::Severity::Error),
                "issues": issues.iter().map(|i| json!({
                    "path": i.path,
                    "severity": if i.severity == edit::Severity::Error { "error" } else { "warning" },
                    "message": i.message,
                })).collect::<Vec<_>>()
            }))
        }
        "tree_edit" => tree_edit(s, &a),
        "tree_undo" | "tree_redo" => {
            let (_, d) = s.doc_mut(args)?;
            let next = if tool == "tree_undo" {
                d.history.undo(&d.tree)
            } else {
                d.history.redo(&d.tree)
            };
            match next {
                Some(t) => {
                    d.tree = t;
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
        "tree_save" => tree_save(s, &a),
        "event_list" => {
            let (_, d) = s.doc_mut(args)?;
            let mut ev: Vec<_> = d.db.events.values().collect();
            ev.sort_by(|x, y| x.id.cmp(&y.id));
            Ok(json!({ "events": ev.iter().map(|e| event_json(e)).collect::<Vec<_>>() }))
        }
        "event_set" => event_set(s, &a),
        "analysis_cutsets" => {
            let (_, d) = s.doc_mut(args)?;
            let sets =
                minimal_cut_sets_limited(&d.tree, a.opt_usize("max_order")?.unwrap_or(usize::MAX))?;
            Ok(
                json!({ "count": sets.len(), "cut_sets": sets.iter().map(cs_json).collect::<Vec<_>>() }),
            )
        }
        "analysis_probability" => {
            let (_, d) = s.doc_mut(args)?;
            let opts = prob_opts(&a)?;
            let r = probability_report(&d.tree, &d.db, &opts)?;
            Ok(prob_json(&r))
        }
        "analysis_montecarlo" => {
            let (_, d) = s.doc_mut(args)?;
            let r = simulate(&d.tree, &d.db, &mc_opts(&a)?)?;
            Ok(mc_json(&r))
        }
        "report" => make_report(s, &a, args),
        other => Err(ToolError::new(
            "unknown_tool",
            format!("no tool named `{other}`"),
        )),
    }
}

// ------------------------------------------------------------- documents

/// Windows canonical paths carry a `\\?\` prefix that is noise in JSON.
fn display_path(p: &Path) -> String {
    let s = p.display().to_string();
    s.strip_prefix(r"\\?\").map(str::to_string).unwrap_or(s)
}

fn read_limited(path: &Path) -> Result<Vec<u8>, ToolError> {
    let meta = std::fs::metadata(path)
        .map_err(|e| ToolError::new("io", format!("{}: {e}", path.display())))?;
    if meta.len() > MAX_FILE {
        return Err(ToolError::new(
            "io",
            format!("{}: file too large", path.display()),
        ));
    }
    std::fs::read(path).map_err(|e| ToolError::new("io", format!("{}: {e}", path.display())))
}

fn tree_open(s: &mut Session, a: &Args) -> Result<Value, ToolError> {
    let path = s.resolve(a.str("path")?)?;
    let raw = a.opt_bool("raw")?.unwrap_or(false);
    let (tree, db) = if raw {
        let tree = parse_fta(&read_limited(&path)?)?;
        let mut db = PrimaryEventDb::default();
        if let Some(name) = &tree.db_name {
            let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
            if let Some(p) = path.parent().map(|d| d.join(base)).filter(|p| p.is_file()) {
                db = parse_ped(&read_limited(&p)?)?;
            }
        }
        (tree, db)
    } else {
        // Project::load reads files itself, so enforce the size limit up front.
        read_limited(&path)?;
        let p = Project::load(&path)?;
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
            dirty: true,
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
        "probability": e.probability, "rate": e.lambda == Some('L'), "group": e.group,
    })
}

// ------------------------------------------------------------- editing

fn tree_edit(s: &mut Session, a: &Args) -> Result<Value, ToolError> {
    let (_, d) = s.doc_mut(a.0)?;
    let op = a.str("op")?;
    // Parse arguments first so a bad request never touches the tree or the history.
    let result: Value = match op {
        "insert_child" => {
            let parent = a.opt_path("parent")?.unwrap_or_default();
            let kind = a.kind("kind")?;
            let mut node = edit::new_node(kind, a.opt_str("id")?.unwrap_or(""));
            if let Some(t) = a.opt_str("text")? {
                node.text = t.to_string();
            }
            let index = a.opt_usize("index")?;
            let p = d.history.edit(&mut d.tree, |t| {
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
            d.history
                .edit(&mut d.tree, |t| edit::delete_subtree(t, &path))?;
            json!({ "deleted": path })
        }
        "move" => {
            let (from, to, index) = (
                a.path("path")?,
                a.path("new_parent")?,
                a.opt_usize("index")?,
            );
            let p = d.history.edit(&mut d.tree, |t| {
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
        "reorder" => {
            let (path, index) = (
                a.path("path")?,
                a.opt_usize("index")?
                    .ok_or_else(|| ToolError::args("missing `index`"))?,
            );
            let p = d
                .history
                .edit(&mut d.tree, |t| edit::reorder(t, &path, index))?;
            json!({ "path": p })
        }
        "change_type" => {
            let (path, kind) = (a.path("path")?, a.kind("kind")?);
            d.history
                .edit(&mut d.tree, |t| edit::change_type(t, &path, kind, None))?;
            json!({ "path": path, "kind": kind_name(kind) })
        }
        "set_id" => {
            let (path, id) = (a.path("path")?, a.str("id")?);
            d.history
                .edit(&mut d.tree, |t| edit::set_id(t, &path, id))?;
            json!({ "path": path, "id": id })
        }
        "set_text" => {
            let (path, text) = (a.path("path")?, a.str("text")?);
            d.history
                .edit(&mut d.tree, |t| edit::set_text(t, &path, text))?;
            json!({ "path": path })
        }
        "set_note" => {
            let note = a.str("note")?;
            d.history
                .edit(&mut d.tree, |t| -> Result<(), edit::EditError> {
                    edit::set_note(t, note);
                    Ok(())
                })?;
            json!({})
        }
        other => return Err(ToolError::args(format!("unknown op `{other}`"))),
    };
    d.dirty = true;
    Ok(result)
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
    if !kind.is_primary() {
        return Err(ToolError::args("kind must be a primary event kind"));
    }
    let old = d.db.events.get(id).cloned();
    let ev = PrimaryEvent {
        id: id.to_string(),
        group: old.as_ref().map(|e| e.group.clone()).unwrap_or_default(),
        kind,
        modifier: old.as_ref().and_then(|e| e.modifier),
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
    d.db.events.insert(ev.id.clone(), ev);
    d.dirty = true;
    Ok(out)
}

fn tree_save(s: &mut Session, a: &Args) -> Result<Value, ToolError> {
    let target = match a.opt_str("path")? {
        Some(p) => Some(s.resolve(p)?),
        None => None,
    };
    let (_, d) = s.doc_mut(a.0)?;
    let path = target
        .or_else(|| d.path.clone())
        .ok_or_else(|| ToolError::args("document has no path; pass `path`"))?;
    std::fs::write(&path, write_fta(&d.tree))
        .map_err(|e| ToolError::new("io", format!("{}: {e}", path.display())))?;
    let mut written = vec![path.display().to_string()];
    if a.opt_bool("ped")?.unwrap_or(false) {
        let name = d
            .tree
            .db_name
            .clone()
            .ok_or_else(|| ToolError::args("tree has no database name (set one first)"))?;
        let base = name.rsplit(['/', '\\']).next().unwrap_or(&name).to_string();
        let ped = path.parent().unwrap_or_else(|| Path::new(".")).join(base);
        std::fs::write(&ped, write_ped(&d.db))
            .map_err(|e| ToolError::new("io", format!("{}: {e}", ped.display())))?;
        written.push(ped.display().to_string());
    }
    d.path = Some(path);
    d.dirty = false;
    Ok(json!({ "written": written }))
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
    json!({
        "trials": r.trials,
        "system_failures": r.system_failures,
        "probability": r.probability,
        "std_error": r.std_error,
        "any_component_failure": r.any_component_failure,
        "cut_set_hits": r.cut_set_hits.iter().map(|(c, n)| json!({ "events": cs_json(c), "hits": n })).collect::<Vec<_>>(),
        "event_failures": r.event_failures.iter().map(|(e, n)| json!({ "id": e, "failures": n })).collect::<Vec<_>>(),
    })
}

fn make_report(s: &mut Session, a: &Args, args: &Value) -> Result<Value, ToolError> {
    let (_, d) = s.doc_mut(args)?;
    let kind = a.str("kind")?;
    let time = a.opt_str("time")?.unwrap_or("");
    let events = report::primary_event_order(&d.tree);
    let text = match kind {
        "cutsets" => {
            let sets =
                minimal_cut_sets_limited(&d.tree, a.opt_usize("max_order")?.unwrap_or(usize::MAX))?;
            report::cut_sets_report(&d.name, time, &events, &sets)
        }
        "probability" => {
            let opts = prob_opts(a)?;
            let all = minimal_cut_sets_limited(&d.tree, usize::MAX)?;
            let max = all.iter().map(CutSet::len).max().unwrap_or(0);
            let r = probability_report(&d.tree, &d.db, &opts)?;
            report::probability_report_text(&d.name, time, &events, max, &opts, &r)
        }
        "montecarlo" | "failure_statistics" => {
            let opts = mc_opts(a)?;
            let r = simulate(&d.tree, &d.db, &opts)?;
            let stats = report::failure_statistics_from_hits(&r, 50);
            if kind == "montecarlo" {
                report::monte_carlo_report_text(&d.name, time, opts.unit_time, &r, &stats)
            } else {
                report::failure_statistics_report(&d.name, time, &stats)
            }
        }
        other => return Err(ToolError::args(format!("unknown report kind `{other}`"))),
    };
    Ok(json!({ "kind": kind, "text": text }))
}
