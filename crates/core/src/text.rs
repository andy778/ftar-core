//! A plain-text fault tree format (`.ftt`), written by hand and diffed in git.
//!
//! The syntax follows the paragraphs of [PFTA](https://github.com/public-fta/pfta)
//! (`Gate: ID` / `Event: ID` headers followed by `- key: value` lines); the
//! meaning is ftar's own, so one file holds a tree together with its primary
//! event database. See `docs/TEXT-FORMAT.md` for the whole format.
//!
//! ```text
//! - note: Failure of an electrical supply to an operating theatre.
//!
//! Gate: Top
//! - label: Electrical power to operating theatre fails.
//! - type: AND
//! - inputs: Backup_Fail, MAINS_FAIL
//!
//! Gate: Backup_Fail
//! - type: OR
//! - inputs: RELAY_FAIL, GEN_FAIL
//!
//! Event: GEN_FAIL
//! - failure_rate: 0.02
//!
//! Event: RELAY_FAIL
//! - probability: 0.05
//!
//! Event: MAINS_FAIL
//! - probability: 0.03
//! ```
//!
//! A `Gate` paragraph is an intermediate event (`label`) with its gate
//! (`type`, optional `gate_id`) and the gate's `inputs`. The first `Gate` is
//! the top event. An `Event` paragraph is a leaf of the tree and, when it has
//! a `probability` or `failure_rate`, an entry in the event database; events
//! that no gate uses are database entries only.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Display;
use std::path::Path;

use crate::database::{validate_event, MAX_TEXT};
use crate::error::{Error, Result};
use crate::model::{EventType, Node, PrimaryEvent, PrimaryEventDb, Tree, MAX_DEPTH};

/// File extension of the text format (without the dot).
pub const EXTENSION: &str = "ftt";

/// Most nodes a text file may expand to. A gate used by several others is
/// copied into each of them, so a small crafted file could otherwise ask for
/// an exponential number of nodes.
pub const MAX_NODES: usize = 100_000;

/// Does `path` name a text-format tree (by its extension)?
pub fn is_text_path(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case(EXTENSION))
}

/// Keys that PFTA reads but ftar does not support yet: a clear error is
/// better than "unknown key" for a file written for PFTA.
const NOT_YET: &[&str] = &[
    "times",
    "time_unit",
    "seed",
    "sample_size",
    "computational_order",
    "computational_tolerance",
    "significant_figures",
    "scientific_exponent",
    "model",
    "model_type",
    "intensity",
    "repair_rate",
    "mean_failure_time",
    "mean_repair_time",
    "appearance",
    "comment",
    "is_paged",
];

fn err(line: usize, msg: impl Display) -> Error {
    Error::Parse(format!("line {line}: {msg}"))
}

// ------------------------------------------------------------- paragraphs

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Header {
    Properties,
    Gate,
    Event,
}

#[derive(Debug)]
struct Paragraph {
    header: Header,
    id: String,
    line: usize,
    /// key -> (values in order, line of the first one)
    fields: BTreeMap<String, (Vec<String>, usize)>,
}

impl Paragraph {
    /// Take the single value of `key`, if present.
    fn take(&mut self, key: &str) -> Result<Option<(String, usize)>> {
        match self.fields.remove(key) {
            None => Ok(None),
            Some((mut v, line)) => {
                if v.len() > 1 {
                    return Err(err(line, format!("`{key}` is given more than once")));
                }
                Ok(v.pop().map(|s| (s, line)))
            }
        }
    }

    /// After all known keys were taken: anything left is an error.
    fn finish(self) -> Result<()> {
        match self.fields.into_iter().next() {
            None => Ok(()),
            Some((k, (_, line))) if NOT_YET.contains(&k.as_str()) => Err(Error::Unsupported(
                format!("line {line}: `{k}` is not supported yet"),
            )),
            Some((k, (_, line))) => Err(err(line, format!("unknown key `{k}`"))),
        }
    }
}

/// Split the text into paragraphs. Blank lines end a paragraph, `#` lines
/// are comments, the first paragraph may be headerless (tree properties).
fn paragraphs(text: &str) -> Result<Vec<Paragraph>> {
    let mut out: Vec<Paragraph> = Vec::new();
    let mut open = false;
    for (i, raw) in text.split('\n').enumerate() {
        let ln = i + 1;
        let line = raw.strip_suffix('\r').unwrap_or(raw).trim_start();
        if line.trim_end().is_empty() {
            open = false;
            continue;
        }
        if line.starts_with('#') {
            continue;
        }
        if let Some(field) = line.strip_prefix('-') {
            let (key, value) = field
                .split_once(':')
                .ok_or_else(|| err(ln, "expected `- key: value`"))?;
            let key = key.trim();
            if key.is_empty() {
                return Err(err(ln, "missing key before `:`"));
            }
            if !open {
                if !out.is_empty() {
                    return Err(err(
                        ln,
                        "field outside a paragraph (start one with `Gate: ID` or `Event: ID`)",
                    ));
                }
                out.push(Paragraph {
                    header: Header::Properties,
                    id: String::new(),
                    line: ln,
                    fields: BTreeMap::new(),
                });
                open = true;
            }
            if let Some(p) = out.last_mut() {
                p.fields
                    .entry(key.to_string())
                    .or_insert_with(|| (Vec::new(), ln))
                    .0
                    .push(value.trim_start().to_string());
            }
            continue;
        }
        let (head, id) = line
            .split_once(':')
            .ok_or_else(|| err(ln, "expected `Gate: ID`, `Event: ID` or `- key: value`"))?;
        let header = match head.trim() {
            "Gate" => Header::Gate,
            "Event" => Header::Event,
            h => return Err(err(ln, format!("unknown paragraph `{h}` (Gate or Event)"))),
        };
        out.push(Paragraph {
            header,
            id: id.trim().to_string(),
            line: ln,
            fields: BTreeMap::new(),
        });
        open = true;
    }
    Ok(out)
}

fn check_id(id: &str, line: usize) -> Result<()> {
    // `NULL` is how a `.fta` file writes "no id".
    if id.is_empty()
        || id == "NULL"
        || id.len() > MAX_TEXT
        || id
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || c == ',' || c == ';')
    {
        return Err(err(
            line,
            format!("bad id `{id}`: ids are required and cannot contain spaces, `,` or `;`"),
        ));
    }
    Ok(())
}

// ------------------------------------------------------------- names

const GATE_NAMES: &[(EventType, &str)] = &[
    (EventType::And, "AND"),
    (EventType::Or, "OR"),
    (EventType::Xor, "XOR"),
    (EventType::Inhibit, "INHIBIT"),
    (EventType::PriorityAnd, "PRIORITY_AND"),
];

const EVENT_NAMES: &[(EventType, &str)] = &[
    (EventType::Basic, "basic"),
    (EventType::Undeveloped, "undeveloped"),
    (EventType::External, "house"),
    (EventType::Dormant, "dormant"),
    (EventType::ConditionAnalysed, "condition"),
    (EventType::ConditionNotAnalysed, "condition_not_analysed"),
    (EventType::TransferIn, "transfer_in"),
];

fn lookup(names: &[(EventType, &'static str)], s: &str) -> Option<EventType> {
    names
        .iter()
        .find(|(_, n)| n.eq_ignore_ascii_case(s))
        .map(|(k, _)| *k)
}

fn name_of(names: &[(EventType, &'static str)], k: EventType) -> Option<&'static str> {
    names.iter().find(|(e, _)| *e == k).map(|(_, n)| *n)
}

fn kind_list(names: &[(EventType, &str)]) -> String {
    names.iter().map(|(_, n)| *n).collect::<Vec<_>>().join(", ")
}

// ------------------------------------------------------------- definitions

#[derive(Debug)]
struct GateDef {
    line: usize,
    label: String,
    /// The intermediate event's own id when it differs from the paragraph's
    /// (`id: X`, or `unnamed: true` for none); the paragraph's id then only
    /// names it inside the file.
    node_id: Option<String>,
    /// The gate under the intermediate event: kind and id (may be empty).
    gate: Option<(EventType, String)>,
    inputs: Vec<String>,
}

#[derive(Debug)]
struct EventDef {
    kind: EventType,
    modifier: Option<char>,
    entry: Option<PrimaryEvent>,
}

fn gate_def(mut p: Paragraph) -> Result<GateDef> {
    let line = p.line;
    let label = p.take("label")?.map(|v| v.0).unwrap_or_default();
    let node_id = match (p.take("id")?, p.take("unnamed")?) {
        (Some(_), Some((_, l))) => return Err(err(l, "give `id` or `unnamed`, not both")),
        (Some((v, l)), None) => {
            check_id(&v, l)?;
            Some(v)
        }
        (None, Some((v, l))) => flag(&v, l, "unnamed")?.then(String::new),
        (None, None) => None,
    };
    let gate_id = p.take("gate_id")?;
    let gate = match p.take("type")? {
        None => {
            if let Some((_, l)) = gate_id {
                return Err(err(l, "`gate_id` needs a `type`"));
            }
            None
        }
        Some((t, l)) => {
            let kind = lookup(GATE_NAMES, &t).ok_or_else(|| {
                err(
                    l,
                    format!("unknown gate type `{t}` ({})", kind_list(GATE_NAMES)),
                )
            })?;
            let id = match gate_id {
                Some((g, gl)) => {
                    check_id(&g, gl)?;
                    g
                }
                None => String::new(),
            };
            Some((kind, id))
        }
    };
    let mut inputs = Vec::new();
    if let Some((list, l)) = p.take("inputs")? {
        if !list.trim().is_empty() {
            for s in list.split(',') {
                let s = s.trim();
                check_id(s, l)?;
                inputs.push(s.to_string());
            }
        }
    }
    p.finish()?;
    Ok(GateDef {
        line,
        label,
        node_id,
        gate,
        inputs,
    })
}

fn flag(v: &str, line: usize, key: &str) -> Result<bool> {
    match v.trim() {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(err(line, format!("`{key}` is `true` or `false`"))),
    }
}

fn number(v: &str, line: usize, key: &str) -> Result<f64> {
    v.trim()
        .parse()
        .map_err(|_| err(line, format!("bad number `{v}` for `{key}`")))
}

fn event_def(mut p: Paragraph) -> Result<EventDef> {
    let kind = match p.take("type")? {
        None => EventType::Basic,
        Some((t, l)) => lookup(EVENT_NAMES, &t).ok_or_else(|| {
            err(
                l,
                format!("unknown event type `{t}` ({})", kind_list(EVENT_NAMES)),
            )
        })?,
    };
    let modifier = match p.take("dormant")? {
        Some((v, l)) if flag(&v, l, "dormant")? => Some('D'),
        _ => None,
    };
    let label = p.take("label")?;
    let group = p.take("group")?;
    let prob = p.take("probability")?;
    let rate = p.take("failure_rate")?;
    let (value, lambda) = match (prob, rate) {
        (Some(_), Some((_, l))) => {
            return Err(err(l, "give `probability` or `failure_rate`, not both"))
        }
        (Some((v, l)), None) => (Some(number(&v, l, "probability")?), None),
        (None, Some((v, l))) => (Some(number(&v, l, "failure_rate")?), Some('L')),
        (None, None) => (None, None),
    };
    let entry = match value {
        Some(probability) => {
            let e = PrimaryEvent {
                id: p.id.clone(),
                group: group.map(|g| g.0).unwrap_or_default(),
                kind,
                modifier,
                description: label.map(|g| g.0).unwrap_or_default(),
                probability,
                lambda,
            };
            validate_event(&e).map_err(|e| err(p.line, e))?;
            Some(e)
        }
        None => {
            if let Some((_, l)) = label.or(group) {
                return Err(err(
                    l,
                    "an event with a `label` or `group` needs a `probability` or `failure_rate`",
                ));
            }
            None
        }
    };
    p.finish()?;
    Ok(EventDef {
        kind,
        modifier,
        entry,
    })
}

// ------------------------------------------------------------- building

struct Builder<'a> {
    gates: &'a HashMap<String, GateDef>,
    events: &'a HashMap<String, EventDef>,
    nodes: usize,
    active: HashSet<String>,
    used: HashSet<String>,
}

impl Builder<'_> {
    fn count(&mut self, line: usize) -> Result<()> {
        self.nodes = self.nodes.saturating_add(1);
        if self.nodes > MAX_NODES {
            return Err(err(
                line,
                format!("the tree expands to more than {MAX_NODES} nodes"),
            ));
        }
        Ok(())
    }

    /// The intermediate event of gate paragraph `id` at `depth`, with its subtree.
    fn gate(&mut self, id: &str, def: &GateDef, depth: usize) -> Result<Node> {
        if depth + 3 > MAX_DEPTH {
            return Err(err(
                def.line,
                format!("tree is nested more than {MAX_DEPTH} levels deep"),
            ));
        }
        if !self.active.insert(id.to_string()) {
            return Err(err(def.line, format!("gate `{id}` is its own input")));
        }
        self.used.insert(id.to_string());
        self.count(def.line)?;
        let child_depth = depth + 1 + usize::from(def.gate.is_some());
        let mut inputs = Vec::with_capacity(def.inputs.len());
        for i in &def.inputs {
            inputs.push(self.input(i, def.line, child_depth)?);
        }
        let children = match &def.gate {
            None => inputs,
            Some((kind, gid)) => {
                self.count(def.line)?;
                vec![leaf(*kind, gid, None, inputs)]
            }
        };
        self.active.remove(id);
        Ok(Node {
            kind: EventType::Intermediate,
            modifier: None,
            id: def.node_id.clone().unwrap_or_else(|| id.to_string()),
            text: def.label.clone(),
            children,
        })
    }

    fn input(&mut self, id: &str, line: usize, depth: usize) -> Result<Node> {
        if let Some(g) = self.gates.get(id) {
            return self.gate(id, g, depth);
        }
        let e = self
            .events
            .get(id)
            .ok_or_else(|| err(line, format!("input `{id}` is not defined")))?;
        self.used.insert(id.to_string());
        self.count(line)?;
        Ok(leaf(e.kind, id, e.modifier, Vec::new()))
    }
}

fn leaf(kind: EventType, id: &str, modifier: Option<char>, children: Vec<Node>) -> Node {
    Node {
        kind,
        modifier,
        id: id.to_string(),
        text: String::new(),
        children,
    }
}

/// Tree properties: the note and the transfer-out marker.
fn properties(mut p: Paragraph) -> Result<(String, Option<String>)> {
    let note = p
        .fields
        .remove("note")
        .map(|(v, _)| v.join("\n"))
        .unwrap_or_default();
    let transfer_out = match p.take("transfer_out")? {
        Some((t, l)) => {
            check_id(&t, l)?;
            Some(t)
        }
        None => None,
    };
    p.finish()?;
    Ok((note, transfer_out))
}

/// Read a text-format tree. Returns the tree (without a database name) and
/// its event database.
pub fn parse_text(input: &[u8]) -> Result<(Tree, PrimaryEventDb)> {
    let text = std::str::from_utf8(input)
        .map_err(|e| Error::Parse(format!("the file is not UTF-8: {e}")))?;
    let mut gates: HashMap<String, GateDef> = HashMap::new();
    let mut events: HashMap<String, EventDef> = HashMap::new();
    let mut top: Option<(String, usize)> = None;
    let mut props = (String::new(), None);
    for p in paragraphs(text)? {
        let (id, line) = (p.id.clone(), p.line);
        match p.header {
            Header::Properties => props = properties(p)?,
            h => {
                check_id(&id, line)?;
                if gates.contains_key(&id) || events.contains_key(&id) {
                    return Err(err(line, format!("`{id}` is defined twice")));
                }
                if h == Header::Gate {
                    top.get_or_insert_with(|| (id.clone(), line));
                    gates.insert(id, gate_def(p)?);
                } else {
                    events.insert(id, event_def(p)?);
                }
            }
        }
    }
    let (top, _) =
        top.ok_or_else(|| Error::Parse("no `Gate:` paragraph (the top event)".into()))?;
    let mut b = Builder {
        gates: &gates,
        events: &events,
        nodes: 0,
        active: HashSet::new(),
        used: HashSet::new(),
    };
    let def = gates
        .get(&top)
        .ok_or_else(|| Error::Parse("top gate missing".into()))?;
    let mut root = b.gate(&top, def, 0)?;
    let mut unused: Vec<(&String, usize)> = gates
        .iter()
        .filter(|(id, _)| !b.used.contains(*id))
        .map(|(id, g)| (id, g.line))
        .collect();
    unused.sort_by_key(|u| u.1);
    if let Some((id, line)) = unused.first() {
        return Err(err(*line, format!("gate `{id}` is not part of the tree")));
    }
    let (note, transfer_out) = props;
    if let Some(t) = transfer_out {
        root.children
            .insert(0, leaf(EventType::TransferOut, &t, None, Vec::new()));
    }
    let mut db = PrimaryEventDb::default();
    for e in events.into_values() {
        if let Some(entry) = e.entry {
            db.events.insert(entry.id.clone(), entry);
        }
    }
    Ok((
        Tree {
            db_name: None,
            note,
            root,
        },
        db,
    ))
}

// ------------------------------------------------------------- writing

fn unsupported(msg: impl Display) -> Error {
    Error::Unsupported(format!("text format: {msg}"))
}

fn write_id(id: &str) -> Result<&str> {
    check_id(id, 0).map_err(|_| unsupported(format!("id `{id}` cannot be written")))?;
    Ok(id)
}

fn single_line(s: &str) -> Result<&str> {
    if s.contains(['\n', '\r']) {
        return Err(unsupported(format!("`{s}` spans several lines")));
    }
    Ok(s)
}

/// Shortest exact form, in exponent notation for very small or large values.
fn format_number(x: f64) -> String {
    let a = x.abs();
    if x == 0.0 || (1e-4..1e6).contains(&a) {
        format!("{x}")
    } else {
        format!("{x:e}")
    }
}

/// An intermediate event still to be written, with the name its paragraph gets.
type Pending<'a> = (&'a Node, String, &'a [Node]);

struct Writer<'a> {
    out: String,
    /// Intermediate events written so far, by id: each different subtree
    /// with the name of its paragraph.
    gates: HashMap<&'a str, Vec<(&'a Node, String)>>,
    events: HashMap<&'a str, &'a Node>,
    event_order: Vec<&'a Node>,
    /// Ids of the events (leaves of the tree, database entries).
    leaf_ids: HashSet<&'a str>,
    /// Every id and paragraph name, so made-up names cannot clash.
    taken: HashSet<String>,
}

impl<'a> Writer<'a> {
    fn new(tree: &'a Tree, db: &'a PrimaryEventDb) -> Self {
        let nodes = tree.nodes();
        let mut leaf_ids: HashSet<&str> = nodes
            .iter()
            .filter(|n| n.kind != EventType::Intermediate && !n.kind.is_gate())
            .map(|n| n.id.as_str())
            .collect();
        leaf_ids.extend(db.events.keys().map(String::as_str));
        let mut taken: HashSet<String> = nodes.iter().map(|n| n.id.clone()).collect();
        taken.extend(db.events.keys().cloned());
        Writer {
            out: String::new(),
            gates: HashMap::new(),
            events: HashMap::new(),
            event_order: Vec::new(),
            leaf_ids,
            taken,
        }
    }

    /// A paragraph name `<base>_<k>` that is no id and no other paragraph's name.
    fn fresh_name(&mut self, base: &str) -> String {
        // An iterator rather than a counter, so no mutation can make it spin.
        (1usize..)
            .map(|k| format!("{base}_{k}"))
            .find(|name| self.taken.insert(name.clone()))
            .unwrap_or_else(|| base.to_string())
    }

    /// The paragraph name of intermediate event `n`, and whether its
    /// paragraph is still to be written. An event keeps its id as the name
    /// unless it has none, an event has the same id, or another subtree
    /// already has it.
    fn gate_name(&mut self, n: &'a Node) -> (String, bool) {
        if n.id.is_empty() {
            return (self.fresh_name("unnamed"), true);
        }
        let seen = self.gates.get(n.id.as_str());
        if let Some((_, name)) = seen.and_then(|v| v.iter().find(|(m, _)| *m == n)) {
            return (name.clone(), false);
        }
        let name = if seen.is_none() && !self.leaf_ids.contains(n.id.as_str()) {
            n.id.clone()
        } else {
            self.fresh_name(&n.id)
        };
        self.gates.entry(&n.id).or_default().push((n, name.clone()));
        (name, true)
    }

    fn line(&mut self, key: &str, value: &str) {
        self.out.push_str("- ");
        self.out.push_str(key);
        self.out.push(':');
        if !value.is_empty() {
            self.out.push(' ');
            self.out.push_str(value);
        }
        self.out.push('\n');
    }

    /// Write the gate paragraph `name` of intermediate event `n` (whose
    /// children are `kids`); returns the intermediate events among its
    /// inputs that are still to be written.
    fn gate(&mut self, n: &'a Node, name: &str, kids: &'a [Node]) -> Result<Vec<Pending<'a>>> {
        if n.modifier.is_some() {
            return Err(unsupported(format!("`{}` has a type marker", n.id)));
        }
        self.out.push_str("\nGate: ");
        self.out.push_str(write_id(name)?);
        self.out.push('\n');
        if n.id.is_empty() {
            self.line("unnamed", "true");
        } else if n.id != name {
            self.line("id", write_id(&n.id)?);
        }
        if !n.text.is_empty() {
            self.line("label", single_line(&n.text)?);
        }
        let inputs = match kids {
            [g] if g.kind.is_gate() => {
                let name = name_of(GATE_NAMES, g.kind).unwrap_or("OR");
                self.line("type", name);
                if !g.id.is_empty() {
                    self.line("gate_id", write_id(&g.id)?);
                }
                if g.modifier.is_some() {
                    return Err(unsupported(format!("gate `{}` has a type marker", g.id)));
                }
                &g.children
            }
            _ => kids,
        };
        let mut names = Vec::with_capacity(inputs.len());
        let mut todo = Vec::new();
        for c in inputs {
            names.push(self.input(n, c, &mut todo)?);
        }
        self.line("inputs", &names.join(", "));
        Ok(todo)
    }

    /// Record input `c` of `parent`; returns the name it is referred to by.
    fn input(&mut self, parent: &Node, c: &'a Node, todo: &mut Vec<Pending<'a>>) -> Result<String> {
        match c.kind {
            EventType::Intermediate => {
                let (name, new) = self.gate_name(c);
                if new {
                    todo.push((c, name.clone(), &c.children));
                }
                Ok(name)
            }
            k if k.is_gate() || k == EventType::TransferOut || k == EventType::Unknown => Err(
                unsupported(format!("`{}` cannot be an input of `{}`", c.id, parent.id)),
            ),
            _ => {
                match self.events.get(c.id.as_str()) {
                    Some(seen) if seen.kind != c.kind || dormant(seen) != dormant(c) => {
                        return Err(unsupported(format!(
                            "event `{}` appears with different types",
                            c.id
                        )))
                    }
                    Some(_) => {}
                    None => {
                        self.events.insert(&c.id, c);
                        self.event_order.push(c);
                    }
                }
                Ok(write_id(&c.id)?.to_string())
            }
        }
    }

    fn event(
        &mut self,
        id: &str,
        kind: EventType,
        dormant: bool,
        e: Option<&PrimaryEvent>,
    ) -> Result<()> {
        self.out.push_str("\nEvent: ");
        self.out.push_str(write_id(id)?);
        self.out.push('\n');
        let name = name_of(EVENT_NAMES, kind)
            .ok_or_else(|| unsupported(format!("event `{id}` has type `{}`", kind.code())))?;
        if kind != EventType::Basic {
            self.line("type", name);
        }
        if dormant {
            self.line("dormant", "true");
        }
        let Some(e) = e else { return Ok(()) };
        if !e.description.is_empty() {
            self.line("label", single_line(&e.description)?);
        }
        if !e.group.is_empty() {
            self.line("group", single_line(&e.group)?);
        }
        if !e.probability.is_finite() {
            return Err(unsupported(format!("event `{id}` has no finite value")));
        }
        let key = if e.lambda == Some('L') {
            "failure_rate"
        } else {
            "probability"
        };
        self.line(key, &format_number(e.probability));
        Ok(())
    }
}

/// Only the dormant marker `D` means something; OpenFTA also wrote a space
/// or NUL for "none".
fn dormant(n: &Node) -> bool {
    n.modifier == Some('D')
}

/// Write a tree and its event database in the text format. The result
/// reads back to the same tree and database, except that markers other
/// than the dormant `D` and the rate `L` (OpenFTA also wrote spaces there)
/// become absent. Fails with [`Error::Unsupported`] for trees the format
/// cannot hold: structures OpenFTA does not allow (a gate directly under a
/// gate), one event id with two types, or ids with spaces or `,`.
pub fn write_text(tree: &Tree, db: &PrimaryEventDb) -> Result<String> {
    let root = &tree.root;
    if root.kind != EventType::Intermediate {
        return Err(unsupported("the top event must be an intermediate event"));
    }
    let mut w = Writer::new(tree, db);
    w.out
        .push_str("# ftar fault tree, text format (docs/TEXT-FORMAT.md)\n");
    let kids = match root.children.split_first() {
        Some((t, rest)) if t.kind == EventType::TransferOut => {
            w.line("transfer_out", write_id(&t.id)?);
            rest
        }
        _ => &root.children[..],
    };
    if !tree.note.is_empty() {
        for l in tree.note.split('\n') {
            w.line("note", l.strip_suffix('\r').unwrap_or(l));
        }
    }
    let (name, _) = w.gate_name(root);
    let mut stack: Vec<Pending> = vec![(root, name, kids)];
    while let Some((n, name, kids)) = stack.pop() {
        let todo = w.gate(n, &name, kids)?;
        stack.extend(todo.into_iter().rev());
    }
    for n in std::mem::take(&mut w.event_order) {
        w.event(&n.id, n.kind, dormant(n), db.events.get(&n.id))?;
    }
    let mut rest: Vec<&PrimaryEvent> = db
        .events
        .values()
        .filter(|e| !w.events.contains_key(e.id.as_str()))
        .collect();
    rest.sort_by(|a, b| a.id.cmp(&b.id));
    for e in rest {
        w.event(&e.id, e.kind, e.modifier == Some('D'), Some(e))?;
    }
    Ok(w.out)
}
