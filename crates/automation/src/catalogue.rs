//! Tool catalogue: names, descriptions and JSON-Schema for the arguments.

use serde_json::{json, Value};

#[derive(Debug, Clone)]
pub struct ToolInfo {
    pub name: &'static str,
    pub description: &'static str,
    /// JSON Schema (draft 2020-12) of the argument object.
    pub input_schema: Value,
}

impl ToolInfo {
    pub fn to_json(&self) -> Value {
        json!({ "name": self.name, "description": self.description, "inputSchema": self.input_schema })
    }
}

fn obj(props: Value, required: &[&str]) -> Value {
    json!({ "type": "object", "properties": props, "required": required, "additionalProperties": false })
}

pub fn tools() -> Vec<ToolInfo> {
    let doc = json!({ "type": "string", "description": "Document id; defaults to the most recently opened document." });
    let path = json!({ "type": "array", "items": { "type": "integer", "minimum": 0 }, "description": "Child indices from the root; [] is the root." });
    let kind = json!({ "type": "string", "description": "Event kind: one-letter file code (A O X P H B U E D M I T C N) or name (and, or, basic, undeveloped, intermediate, condition, ...)." });
    let order = json!({ "type": "integer", "minimum": 1, "description": "Use cut sets up to this order (default: all)." });
    let unit = json!({ "type": "number", "minimum": 0, "description": "Unit time for failure-rate events (default 1)." });
    let t = |name, description, input_schema| ToolInfo {
        name,
        description,
        input_schema,
    };
    vec![
        t("tree_open", "Open a .fta tree with its .ped database. Transfer-in trees are inlined unless raw=true (use raw for editing).",
          obj(json!({ "path": {"type":"string"}, "raw": {"type":"boolean"} }), &["path"])),
        t("tree_new", "Create an empty tree (an intermediate top event with one OR gate).",
          obj(json!({ "name": {"type":"string"} }), &[])),
        t("tree_close", "Close a document.", obj(json!({ "doc": doc }), &[])),
        t("tree_list", "List open documents.", obj(json!({}), &[])),
        t("tree_info", "Summary of a document: counts, note, database name.", obj(json!({ "doc": doc }), &[])),
        t("tree_get", "Return the node at `path` as JSON, down to `depth` levels (default: whole subtree).",
          obj(json!({ "doc": doc, "path": path, "depth": {"type":"integer","minimum":0} }), &[])),
        t("tree_validate", "Structural problems that would stop an analysis (child counts, missing ids).",
          obj(json!({ "doc": doc }), &[])),
        t("tree_edit", "Edit the tree. `op` is one of insert_child, delete, move, reorder, change_type, set_id, set_text, set_note. Each edit is undoable.",
          obj(json!({
            "doc": doc,
            "op": {"type":"string","enum":["insert_child","delete","move","reorder","change_type","set_id","set_text","set_note"]},
            "parent": path, "index": {"type":"integer","minimum":0}, "path": path, "new_parent": path,
            "kind": kind, "id": {"type":"string"}, "text": {"type":"string"}, "note": {"type":"string"}
          }), &["op"])),
        t("tree_undo", "Undo the last edit.", obj(json!({ "doc": doc }), &[])),
        t("tree_redo", "Redo the last undone edit.", obj(json!({ "doc": doc }), &[])),
        t("tree_save", "Write the tree as .fta (and the database as .ped when `ped` is true).",
          obj(json!({ "doc": doc, "path": {"type":"string"}, "ped": {"type":"boolean"} }), &[])),
        t("tree_export", "Draw the tree to an image file: png (default scale 2), svg or a printable html page. The format comes from `format` or the file extension.",
          obj(json!({ "doc": doc, "path": {"type":"string"}, "format": {"type":"string","enum":["png","svg","html"]},
                      "scale": {"type":"number","exclusiveMinimum":0}, "title": {"type":"string"}, "probabilities": {"type":"boolean"} }), &["path"])),
        t("event_list", "List primary events in the database.", obj(json!({ "doc": doc }), &[])),
        t("event_set", "Create or update a primary event in the database. `rate` marks `probability` as a failure rate (lambda).",
          obj(json!({ "doc": doc, "id": {"type":"string"}, "probability": {"type":"number","minimum":0},
                      "description": {"type":"string"}, "rate": {"type":"boolean"}, "kind": kind }), &["id", "probability"])),
        t("analysis_cutsets", "Minimal cut sets, sorted by order.",
          obj(json!({ "doc": doc, "max_order": order }), &[])),
        t("analysis_probability", "Cut-set probabilities, top-event probability (inclusion-exclusion with bounds) and event importance.",
          obj(json!({ "doc": doc, "max_order": order, "terms": {"type":"integer","minimum":1}, "unit_time": unit }), &[])),
        t("analysis_montecarlo", "Monte Carlo simulation; deterministic for a given seed.",
          obj(json!({ "doc": doc, "trials": {"type":"integer","minimum":1}, "seed": {"type":"integer","minimum":0}, "unit_time": unit,
                      "method": {"type":"string","enum":["first_failure","direct"],"description":"first_failure (default, OpenFTA's fast method) or direct."} }), &[])),
        t("report", "OpenFTA-style text report. kind: cutsets, probability, montecarlo, failure_statistics.",
          obj(json!({ "doc": doc, "kind": {"type":"string","enum":["cutsets","probability","montecarlo","failure_statistics"]},
                      "max_order": order, "terms": {"type":"integer","minimum":1}, "unit_time": unit,
                      "trials": {"type":"integer","minimum":1}, "seed": {"type":"integer","minimum":0},
                      "method": {"type":"string","enum":["first_failure","direct"]},
                      "time": {"type":"string","description":"Timestamp text for the header (default empty, for reproducible output)."} }), &["kind"])),
    ]
}
