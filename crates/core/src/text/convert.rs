//! Between the paragraphs of a file ([`Ast`]) and ftar's tree, event
//! database and [`Extras`] (what the tree cannot hold: models, comments,
//! VOTE gates, more top gates, quantification settings).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Display;

use super::ast::{
    err, valid_id, Ast, EventDef, EventModel, GateDef, GateType, Model, ModelDef, Param, Props,
    Rate,
};
use crate::database::validate_event;
use crate::error::{Error, Result};
use crate::model::{EventType, Node, PrimaryEvent, PrimaryEventDb, Tree, MAX_DEPTH};

/// Most nodes a text file may expand to. A gate used by several others is
/// copied into each of them, and a VOTE gate into each combination of its
/// inputs, so a small crafted file could otherwise ask for an exponential
/// number of nodes.
pub const MAX_NODES: usize = 100_000;

/// What a text file holds beyond the tree and its event database.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Extras {
    /// Tree properties, except the note and transfer-out (in the tree).
    pub props: Props,
    pub models: Vec<ModelDef>,
    /// By event id.
    pub events: BTreeMap<String, EventExtra>,
    /// By the id of the intermediate event.
    pub gates: BTreeMap<String, GateExtra>,
    /// Gates that are not under the top event (PFTA allows several top
    /// gates), as written.
    pub detached: Vec<GateDef>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EventExtra {
    pub kind: EventType,
    /// The label, when the event database cannot hold it.
    pub label: Option<String>,
    pub comment: Option<String>,
    /// The model as written. Used while the database entry still says the
    /// same; an edited probability replaces it.
    pub model: Option<EventModel>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct GateExtra {
    pub comment: Option<String>,
    pub is_paged: bool,
    /// A VOTE gate: threshold and inputs. The tree holds its expansion.
    pub vote: Option<(usize, Vec<String>)>,
}

fn unsupported(msg: impl Display) -> Error {
    Error::Unsupported(format!("text format: {msg}"))
}

// ------------------------------------------------------------- models

fn point(p: &Param) -> Option<f64> {
    match p {
        Param::Value(x) => Some(*x),
        Param::Dist(_) => None,
    }
}

fn rate_value(r: &Rate) -> Option<f64> {
    match r {
        Rate::Rate(p) => point(p),
        Rate::MeanTime(p) => point(p).map(|m| if m == 0.0 { f64::INFINITY } else { 1.0 / m }),
    }
}

/// The model of an event, following a reference to a shared model.
pub(crate) fn resolve<'a>(m: &'a EventModel, models: &'a [ModelDef]) -> Option<&'a Model> {
    match m {
        EventModel::Own(m) => Some(m),
        EventModel::Shared(id) => models.iter().find(|d| d.id == *id).map(|d| &d.model),
    }
}

/// What OpenFTA's event database can say about a model: a probability, or a
/// failure rate (`L`) without repair. `None` for everything else
/// (distributions, repair, intensities on their own, `nan`).
pub(crate) fn database_value(m: &Model) -> Option<(f64, Option<char>)> {
    match m {
        Model::Fixed { probability, .. } => point(probability)
            .filter(|p| (0.0..=1.0).contains(p))
            .map(|p| (p, None)),
        Model::ConstantRate { failure, repair } => {
            let l = rate_value(failure).filter(|l| l.is_finite() && *l >= 0.0)?;
            (rate_value(repair) == Some(0.0)).then_some((l, Some('L')))
        }
        Model::True => Some((1.0, None)),
        Model::False => Some((0.0, None)),
    }
}

fn entry_value(e: &PrimaryEvent) -> (f64, Option<char>) {
    (e.probability, e.lambda.filter(|l| *l == 'L'))
}

// ------------------------------------------------------------- Ast -> tree

struct Builder<'a> {
    gates: &'a HashMap<&'a str, &'a GateDef>,
    events: &'a HashMap<&'a str, &'a EventDef>,
    nodes: usize,
    active: HashSet<String>,
    used: HashSet<String>,
}

fn node_count(n: &Node) -> usize {
    let mut count = 0;
    let mut stack = vec![n];
    while let Some(n) = stack.pop() {
        count += 1;
        stack.extend(&n.children);
    }
    count
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

/// k-element subsets of `0..n`, in lexicographic order.
pub(crate) fn combinations(n: usize, k: usize) -> Vec<Vec<usize>> {
    let mut out = Vec::new();
    if k == 0 || k > n {
        return out;
    }
    let mut c: Vec<usize> = (0..k).collect();
    loop {
        out.push(c.clone());
        let Some(i) = (0..k)
            .rev()
            .find(|&i| c.get(i).is_some_and(|&x| x < n - k + i))
        else {
            return out;
        };
        if let Some(x) = c.get_mut(i) {
            *x += 1;
        }
        for j in i + 1..k {
            let prev = c.get(j - 1).copied().unwrap_or(0);
            if let Some(x) = c.get_mut(j) {
                *x = prev + 1;
            }
        }
    }
}

/// The children of an intermediate event for a gate of type `t` over
/// `inputs`. A VOTE(k) gate becomes an OR of ANDs over every k inputs (an
/// AND when k is all of them, an OR when k is 1).
pub(crate) fn gate_children(t: GateType, gate_id: &str, inputs: Vec<Node>) -> Result<Vec<Node>> {
    Ok(match t {
        GateType::Null => inputs,
        GateType::Vote(k) if k == 0 || k > inputs.len() => {
            return Err(unsupported(format!(
                "VOTE({k}) over {} inputs is always true or never true",
                inputs.len()
            )))
        }
        GateType::Vote(1) => vec![leaf(EventType::Or, gate_id, None, inputs)],
        GateType::Vote(k) if k == inputs.len() => {
            vec![leaf(EventType::And, gate_id, None, inputs)]
        }
        GateType::Vote(k) => {
            let ands = combinations(inputs.len(), k)
                .into_iter()
                .map(|c| Node {
                    kind: EventType::Intermediate,
                    modifier: None,
                    id: String::new(),
                    text: String::new(),
                    children: vec![leaf(
                        EventType::And,
                        "",
                        None,
                        c.iter().filter_map(|&i| inputs.get(i).cloned()).collect(),
                    )],
                })
                .collect();
            vec![leaf(EventType::Or, gate_id, None, ands)]
        }
        t => {
            let kind = t.openfta_kind().unwrap_or(EventType::Or);
            vec![leaf(kind, gate_id, None, inputs)]
        }
    })
}

/// Nodes in the expansion of a gate (for the size check).
fn expansion_size(t: GateType, n: usize) -> usize {
    match t {
        GateType::Vote(k) if k > 1 && k < n => {
            let mut c: usize = 1;
            for i in 0..k {
                c = c.saturating_mul(n - i) / (i + 1);
                if c > MAX_NODES {
                    return usize::MAX;
                }
            }
            c.saturating_mul(2 + k)
        }
        _ => 1,
    }
}

impl Builder<'_> {
    fn count(&mut self, line: usize, n: usize) -> Result<()> {
        self.nodes = self.nodes.saturating_add(n);
        if self.nodes > MAX_NODES {
            return Err(err(
                line,
                format!("the tree expands to more than {MAX_NODES} nodes"),
            ));
        }
        Ok(())
    }

    /// The intermediate event of gate paragraph `def` at `depth`, with its subtree.
    fn gate(&mut self, def: &GateDef, depth: usize) -> Result<Node> {
        let id = def.id.as_str();
        let extra = usize::from(
            matches!(def.gate_type, GateType::Vote(k) if k > 1 && k < def.inputs.len()),
        ) * 2;
        if depth + 3 + extra > MAX_DEPTH {
            return Err(err(
                def.line,
                format!("tree is nested more than {MAX_DEPTH} levels deep"),
            ));
        }
        if !self.active.insert(id.to_string()) {
            return Err(err(def.line, format!("gate `{id}` is its own input")));
        }
        self.used.insert(id.to_string());
        self.count(def.line, 1)?;
        let child_depth = depth + 1 + usize::from(def.gate_type != GateType::Null) + extra;
        let mut inputs = Vec::with_capacity(def.inputs.len());
        for i in &def.inputs {
            inputs.push(self.input(i, def.line, child_depth)?);
        }
        self.count(def.line, expansion_size(def.gate_type, inputs.len()))?;
        if let GateType::Vote(k) = def.gate_type {
            if k > 1 && k < inputs.len() {
                // Each input is copied into the combinations it is part of.
                let copies: usize = inputs.iter().map(node_count).sum();
                self.count(def.line, copies.saturating_mul(k))?;
            }
        }
        let children = gate_children(def.gate_type, def.gate_id.as_deref().unwrap_or(""), inputs)
            .map_err(|e| err(def.line, e))?;
        self.active.remove(id);
        Ok(Node {
            kind: EventType::Intermediate,
            modifier: None,
            id: def.node_id.clone().unwrap_or_else(|| id.to_string()),
            text: def.label.clone().unwrap_or_default(),
            children,
        })
    }

    fn input(&mut self, id: &str, line: usize, depth: usize) -> Result<Node> {
        if let Some(g) = self.gates.get(id) {
            return self.gate(g, depth);
        }
        let e = self
            .events
            .get(id)
            .ok_or_else(|| err(line, format!("input `{id}` is not defined")))?;
        self.used.insert(id.to_string());
        self.count(line, 1)?;
        Ok(leaf(e.kind, id, e.dormant.then_some('D'), Vec::new()))
    }
}

/// The event database entry and extras of one event.
fn event_parts(
    e: &EventDef,
    models: &[ModelDef],
) -> Result<(Option<PrimaryEvent>, Option<EventExtra>)> {
    if let Some(EventModel::Shared(m)) = &e.model {
        if !models.iter().any(|d| d.id == *m) {
            return Err(err(e.line, format!("no model `{m}`")));
        }
    }
    let value = e
        .model
        .as_ref()
        .and_then(|m| resolve(m, models))
        .and_then(database_value);
    let mut entry = value.map(|(probability, lambda)| PrimaryEvent {
        id: e.id.clone(),
        group: e.group.clone().unwrap_or_default(),
        kind: e.kind,
        modifier: e.dormant.then_some('D'),
        description: e.label.clone().unwrap_or_default(),
        probability,
        lambda,
    });
    let mut label_extra = None;
    if let Some(p) = &mut entry {
        if validate_event(p).is_err() {
            // The database cannot hold it (a `;` in the label, a kind
            // without database entries): keep the label here.
            p.description.clear();
            if validate_event(p).is_err() {
                entry = None;
            }
            label_extra.clone_from(&e.label);
        }
    } else {
        if e.group.is_some() {
            return Err(err(
                e.line,
                "an event with a `group` needs a probability or failure rate",
            ));
        }
        label_extra.clone_from(&e.label);
    }
    let extra =
        (e.model.is_some() || e.comment.is_some() || label_extra.is_some()).then(|| EventExtra {
            kind: e.kind,
            label: label_extra,
            comment: e.comment.clone(),
            model: e.model.clone(),
        });
    Ok((entry, extra))
}

/// Build ftar's tree, event database and extras from a file's paragraphs.
/// The top event is the first gate no other gate uses.
pub fn to_document(ast: &Ast) -> Result<(Tree, PrimaryEventDb, Extras)> {
    let gates: HashMap<&str, &GateDef> = ast.gates.iter().map(|g| (g.id.as_str(), g)).collect();
    let events: HashMap<&str, &EventDef> = ast.events.iter().map(|e| (e.id.as_str(), e)).collect();
    let inputs: HashSet<&str> = ast
        .gates
        .iter()
        .flat_map(|g| g.inputs.iter().map(String::as_str))
        .collect();
    let mut b = Builder {
        gates: &gates,
        events: &events,
        nodes: 0,
        active: HashSet::new(),
        used: HashSet::new(),
    };
    let top = ast
        .gates
        .iter()
        .find(|g| !inputs.contains(g.id.as_str()))
        .or_else(|| ast.gates.first());
    let mut root = match top {
        Some(g) => b.gate(g, 0)?,
        None => leaf(EventType::Intermediate, "", None, Vec::new()),
    };
    let mut extras = Extras {
        props: Props {
            note: None,
            transfer_out: None,
            ..ast.props.clone()
        },
        models: ast.models.clone(),
        ..Extras::default()
    };
    // Gates not under the top event: check them too, keep them as written.
    let reached = b.used.clone();
    for g in &ast.gates {
        if !reached.contains(&g.id) {
            b.gate(g, 0)?;
            extras.detached.push(g.clone());
        }
    }
    if let Some(t) = &ast.props.transfer_out {
        root.children
            .insert(0, leaf(EventType::TransferOut, t, None, Vec::new()));
    }
    for g in &ast.gates {
        let id = g.node_id.as_deref().unwrap_or(&g.id);
        let vote = match g.gate_type {
            GateType::Vote(k) => Some((k, g.inputs.clone())),
            _ => None,
        };
        if !id.is_empty() && (g.comment.is_some() || g.is_paged || vote.is_some()) {
            extras.gates.insert(
                id.to_string(),
                GateExtra {
                    comment: g.comment.clone(),
                    is_paged: g.is_paged,
                    vote,
                },
            );
        }
    }
    let mut db = PrimaryEventDb::default();
    for e in &ast.events {
        let (entry, extra) = event_parts(e, &ast.models)?;
        if let Some(entry) = entry {
            db.events.insert(e.id.clone(), entry);
        }
        if let Some(extra) = extra {
            extras.events.insert(e.id.clone(), extra);
        }
    }
    let tree = Tree {
        db_name: None,
        note: ast.props.note.clone().unwrap_or_default(),
        root,
    };
    Ok((tree, db, extras))
}

/// The tree of every gate of a file, in file order (for quantifying each).
pub(crate) fn gate_trees(ast: &Ast) -> Result<Vec<Node>> {
    let gates: HashMap<&str, &GateDef> = ast.gates.iter().map(|g| (g.id.as_str(), g)).collect();
    let events: HashMap<&str, &EventDef> = ast.events.iter().map(|e| (e.id.as_str(), e)).collect();
    let mut nodes = 0usize;
    let mut out = Vec::with_capacity(ast.gates.len());
    for g in &ast.gates {
        let mut b = Builder {
            gates: &gates,
            events: &events,
            nodes,
            active: HashSet::new(),
            used: HashSet::new(),
        };
        out.push(b.gate(g, 0)?);
        nodes = b.nodes;
    }
    Ok(out)
}

// ------------------------------------------------------------- tree -> Ast

/// An intermediate event still to be written, with the name its paragraph gets.
type Pending<'a> = (&'a Node, String, &'a [Node]);

struct Writer<'a> {
    ast: Ast,
    extras: &'a Extras,
    db: &'a PrimaryEventDb,
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

fn write_id(id: &str) -> Result<&str> {
    if valid_id(id) {
        Ok(id)
    } else {
        Err(unsupported(format!("id `{id}` cannot be written")))
    }
}

fn single_line(s: &str) -> Result<String> {
    if s.contains(['\n', '\r']) {
        return Err(unsupported(format!("`{s}` spans several lines")));
    }
    Ok(s.to_string())
}

fn non_empty(s: &str) -> Result<Option<String>> {
    if s.is_empty() {
        Ok(None)
    } else {
        single_line(s).map(Some)
    }
}

/// Only the dormant marker `D` means something; OpenFTA also wrote a space
/// or NUL for "none".
fn dormant(n: &Node) -> bool {
    n.modifier == Some('D')
}

impl<'a> Writer<'a> {
    fn new(tree: &'a Tree, db: &'a PrimaryEventDb, extras: &'a Extras) -> Self {
        let nodes = tree.nodes();
        let mut leaf_ids: HashSet<&str> = nodes
            .iter()
            .filter(|n| n.kind != EventType::Intermediate && !n.kind.is_gate())
            .map(|n| n.id.as_str())
            .collect();
        leaf_ids.extend(db.events.keys().map(String::as_str));
        leaf_ids.extend(extras.events.keys().map(String::as_str));
        let mut taken: HashSet<String> = nodes.iter().map(|n| n.id.clone()).collect();
        taken.extend(db.events.keys().cloned());
        taken.extend(extras.events.keys().cloned());
        taken.extend(extras.models.iter().map(|m| m.id.clone()));
        taken.extend(extras.detached.iter().map(|g| g.id.clone()));
        Writer {
            ast: Ast::default(),
            extras,
            db,
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

    /// The VOTE gate `n` was read from, when the tree still holds exactly
    /// its expansion: its threshold and input nodes.
    fn vote(&self, n: &'a Node, kids: &'a [Node]) -> Option<(usize, Vec<&'a Node>)> {
        let (k, names) = self.extras.gates.get(&n.id)?.vote.as_ref()?;
        let [g] = kids else { return None };
        let mut found: Vec<Option<&Node>> = vec![None; names.len()];
        let mut visit = |c: &'a Node| {
            if let Some(i) = names.iter().position(|x| *x == c.id) {
                if let Some(slot) = found.get_mut(i) {
                    slot.get_or_insert(c);
                }
            }
        };
        g.children.iter().for_each(&mut visit);
        for c in &g.children {
            for a in &c.children {
                a.children.iter().for_each(&mut visit);
            }
        }
        let inputs: Vec<&Node> = found.into_iter().collect::<Option<_>>()?;
        let expect = gate_children(
            GateType::Vote(*k),
            &g.id,
            inputs.iter().map(|c| (*c).clone()).collect(),
        )
        .ok()?;
        (expect.as_slice() == kids).then_some((*k, inputs))
    }

    /// Add the gate paragraph `name` of intermediate event `n` (whose
    /// children are `kids`); returns the intermediate events among its
    /// inputs that are still to be written.
    fn gate(&mut self, n: &'a Node, name: &str, kids: &'a [Node]) -> Result<Vec<Pending<'a>>> {
        if n.modifier.is_some() {
            return Err(unsupported(format!("`{}` has a type marker", n.id)));
        }
        let extra = self.extras.gates.get(&n.id).filter(|_| !n.id.is_empty());
        let mut def = GateDef {
            id: write_id(name)?.to_string(),
            line: 0,
            label: non_empty(&n.text)?,
            comment: extra.and_then(|g| g.comment.clone()),
            is_paged: extra.is_some_and(|g| g.is_paged),
            gate_type: GateType::Null,
            gate_id: None,
            node_id: if n.id.is_empty() {
                Some(String::new())
            } else {
                (n.id != name).then(|| n.id.clone())
            },
            inputs: Vec::new(),
        };
        let inputs: Vec<&'a Node> = if let Some((k, ins)) = self.vote(n, kids) {
            def.gate_type = GateType::Vote(k);
            ins
        } else {
            match kids {
                [g] if g.kind.is_gate() => {
                    if g.modifier.is_some() {
                        return Err(unsupported(format!("gate `{}` has a type marker", g.id)));
                    }
                    def.gate_type = GateType::from_openfta(g.kind).unwrap_or(GateType::Or);
                    if !g.id.is_empty() {
                        def.gate_id = Some(write_id(&g.id)?.to_string());
                    }
                    g.children.iter().collect()
                }
                _ => kids.iter().collect(),
            }
        };
        let mut todo = Vec::new();
        for c in inputs {
            let name = self.input(n, c, &mut todo)?;
            def.inputs.push(name);
        }
        self.ast.gates.push(def);
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

    /// The event paragraph of `id`: the model as read while the database
    /// still says the same, else what the database says.
    fn event(&self, id: &str, kind: EventType, dormant: bool) -> Result<EventDef> {
        let entry = self.db.events.get(id);
        let extra = self.extras.events.get(id);
        let stored = extra.and_then(|x| x.model.as_ref()).filter(|m| {
            resolve(m, &self.extras.models).and_then(database_value) == entry.map(entry_value)
        });
        let model = match (stored, entry) {
            (Some(m), _) => Some(m.clone()),
            (None, Some(e)) => Some(EventModel::Own(if e.lambda == Some('L') {
                Model::ConstantRate {
                    failure: Rate::Rate(Param::Value(e.probability)),
                    repair: Rate::Rate(Param::Value(0.0)),
                }
            } else {
                Model::Fixed {
                    probability: Param::Value(e.probability),
                    intensity: Param::Value(0.0),
                }
            })),
            (None, None) => None,
        };
        if let Some(Param::Value(x)) = match &model {
            Some(EventModel::Own(Model::Fixed { probability, .. })) => Some(probability),
            _ => None,
        } {
            if x.is_infinite() {
                return Err(unsupported(format!(
                    "event `{id}` has an infinite probability"
                )));
            }
        }
        let label = match entry.map(|e| e.description.as_str()) {
            Some(d) if !d.is_empty() => Some(single_line(d)?),
            _ => extra.and_then(|x| x.label.clone()),
        };
        Ok(EventDef {
            id: write_id(id)?.to_string(),
            line: 0,
            label,
            comment: extra.and_then(|x| x.comment.clone()),
            kind,
            dormant,
            group: match entry.map(|e| e.group.as_str()) {
                Some(g) if !g.is_empty() => Some(single_line(g)?),
                _ => None,
            },
            model,
        })
    }
}

/// Build a file's paragraphs from ftar's tree, event database and extras.
/// Fails with [`Error::Unsupported`] for trees the format cannot hold:
/// structures OpenFTA does not allow (a gate directly under a gate), one
/// event id with two types, or ids with spaces or `,`.
pub fn from_document(tree: &Tree, db: &PrimaryEventDb, extras: &Extras) -> Result<Ast> {
    let root = &tree.root;
    if root.kind != EventType::Intermediate {
        return Err(unsupported("the top event must be an intermediate event"));
    }
    let mut w = Writer::new(tree, db, extras);
    w.ast.props = Props {
        note: (!tree.note.is_empty()).then(|| tree.note.clone()),
        transfer_out: None,
        ..extras.props.clone()
    };
    w.ast.props.times.get_or_insert_with(|| vec![1.0]);
    w.ast.models = extras.models.clone();
    let kids = match root.children.split_first() {
        Some((t, rest)) if t.kind == EventType::TransferOut => {
            w.ast.props.transfer_out = Some(write_id(&t.id)?.to_string());
            rest
        }
        _ => &root.children[..],
    };
    // An empty tree (as PFTA's empty file) has no gate paragraph.
    let empty = root.id.is_empty() && root.text.is_empty() && kids.is_empty();
    if !empty {
        let (name, _) = w.gate_name(root);
        let mut stack: Vec<Pending> = vec![(root, name, kids)];
        while let Some((n, name, kids)) = stack.pop() {
            let todo = w.gate(n, &name, kids)?;
            stack.extend(todo.into_iter().rev());
        }
    }
    let written: HashSet<String> = w.ast.gates.iter().map(|g| g.id.clone()).collect();
    for g in &extras.detached {
        if !written.contains(&g.id) {
            w.ast.gates.push(g.clone());
        }
    }
    let gate_names: HashSet<String> = w.ast.gates.iter().map(|g| g.id.clone()).collect();
    for n in std::mem::take(&mut w.event_order) {
        if gate_names.contains(&n.id) {
            return Err(unsupported(format!(
                "`{}` is both a gate and an event",
                n.id
            )));
        }
        let e = w.event(&n.id, n.kind, dormant(n))?;
        w.ast.events.push(e);
    }
    // Events that are not in the tree: database entries, events of the
    // other top gates, unused events.
    let mut rest: BTreeMap<&str, (EventType, bool)> = BTreeMap::new();
    for (id, x) in &extras.events {
        rest.insert(id, (x.kind, false));
    }
    for e in db.events.values() {
        rest.insert(&e.id, (e.kind, e.modifier == Some('D')));
    }
    for (id, (kind, d)) in rest {
        if w.events.contains_key(id) {
            continue;
        }
        if gate_names.contains(id) {
            return Err(unsupported(format!("`{id}` is both a gate and an event")));
        }
        let e = w.event(id, kind, d)?;
        w.ast.events.push(e);
    }
    check_references(&w.ast)?;
    Ok(w.ast)
}

/// Every input and model reference must name a paragraph.
fn check_references(ast: &Ast) -> Result<()> {
    let names: HashSet<&str> = ast
        .gates
        .iter()
        .map(|g| g.id.as_str())
        .chain(ast.events.iter().map(|e| e.id.as_str()))
        .collect();
    for g in &ast.gates {
        if let Some(i) = g.inputs.iter().find(|i| !names.contains(i.as_str())) {
            return Err(unsupported(format!(
                "gate `{}` uses `{i}`, which is no longer defined",
                g.id
            )));
        }
    }
    for e in &ast.events {
        if let Some(EventModel::Shared(m)) = &e.model {
            if !ast.models.iter().any(|d| d.id == *m) {
                return Err(unsupported(format!(
                    "event `{}` uses unknown model `{m}`",
                    e.id
                )));
            }
        }
    }
    Ok(())
}
