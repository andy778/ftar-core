//! The text format as written: paragraphs of tree properties, models, events
//! and gates ([`Ast`]), read from text with [`parse_ast`] and written with
//! [`render`]. Checks keys, values and model key combinations the way PFTA
//! does; references between paragraphs are checked when the tree is built
//! ([`super::convert`]).

use std::collections::{BTreeMap, HashSet};
use std::fmt::Display;

use crate::database::MAX_TEXT;
use crate::error::{Error, Result};
use crate::model::EventType;

pub(crate) fn err(line: usize, msg: impl Display) -> Error {
    Error::Parse(format!("line {line}: {msg}"))
}

// ------------------------------------------------------------- types

/// A whole file.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Ast {
    pub props: Props,
    pub models: Vec<ModelDef>,
    pub gates: Vec<GateDef>,
    pub events: Vec<EventDef>,
}

/// Tree properties (the first paragraph, without a header).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Props {
    /// The tree's note (ftar; one `note` line per line of text).
    pub note: Option<String>,
    /// Transfer-out marker under the top event (ftar).
    pub transfer_out: Option<String>,
    /// Times to quantify at (PFTA requires them; `nan` means "any time").
    pub times: Option<Vec<f64>>,
    pub time_unit: Option<String>,
    pub seed: Option<String>,
    pub sample_size: Option<usize>,
    /// Truncation order of the inclusion-exclusion expansions.
    pub computational_order: Option<usize>,
    /// Stop an expansion once a term is smaller than this share of the sum.
    pub computational_tolerance: Option<f64>,
    /// Drawing options of PFTA; kept so a file is written back unchanged.
    pub significant_figures: Option<u32>,
    pub scientific_exponent: Option<u32>,
}

/// A parameter: a point value or a distribution to sample.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Param {
    Value(f64),
    Dist(Distribution),
}

/// Distributions, parameterised as in PFTA.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Distribution {
    Beta {
        alpha: f64,
        beta: f64,
    },
    /// Shape `alpha`, rate `lambda` (mean `alpha / lambda`).
    Gamma {
        alpha: f64,
        lambda: f64,
    },
    /// `mu` and `sigma` of the underlying normal distribution.
    LogNormal {
        mu: f64,
        sigma: f64,
    },
    LogUniform {
        lower: f64,
        upper: f64,
    },
    Normal {
        mu: f64,
        sigma: f64,
    },
    Triangular {
        lower: f64,
        upper: f64,
        mode: f64,
    },
    Uniform {
        lower: f64,
        upper: f64,
    },
}

/// A failure or repair rate, given as a rate or as a mean time (its inverse).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Rate {
    Rate(Param),
    MeanTime(Param),
}

/// How an event fails over time.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Model {
    /// Constant probability `q` and intensity `ω`.
    Fixed {
        probability: Param,
        intensity: Param,
    },
    /// Constant failure rate λ and repair rate μ:
    /// `q(t) = λ/(λ+μ) (1 - e^-(λ+μ)t)`, `ω(t) = λ (1 - q(t))`.
    ConstantRate { failure: Rate, repair: Rate },
    /// Always failed.
    True,
    /// Never failed.
    False,
}

/// `Model: ID`, a model shared by events.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelDef {
    pub id: String,
    pub line: usize,
    pub label: Option<String>,
    pub comment: Option<String>,
    pub model: Model,
}

/// An event's model: its own, or a `Model` paragraph's.
#[derive(Debug, Clone, PartialEq)]
pub enum EventModel {
    Own(Model),
    Shared(String),
}

/// `Event: ID`.
#[derive(Debug, Clone, PartialEq)]
pub struct EventDef {
    pub id: String,
    pub line: usize,
    pub label: Option<String>,
    pub comment: Option<String>,
    /// Basic, Undeveloped, House (External) or an OpenFTA kind.
    pub kind: EventType,
    pub dormant: bool,
    pub group: Option<String>,
    pub model: Option<EventModel>,
}

/// Gate types: PFTA's and OpenFTA's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateType {
    /// No gate: the inputs hang directly under the intermediate event
    /// (PFTA's `NULL`, which has exactly one input).
    Null,
    And,
    Or,
    /// At least `k` of the inputs.
    Vote(usize),
    Xor,
    Inhibit,
    PriorityAnd,
}

/// `Gate: ID`: an intermediate event with its gate.
#[derive(Debug, Clone, PartialEq)]
pub struct GateDef {
    pub id: String,
    pub line: usize,
    pub label: Option<String>,
    pub comment: Option<String>,
    pub is_paged: bool,
    pub gate_type: GateType,
    /// Id of the gate symbol (OpenFTA).
    pub gate_id: Option<String>,
    /// The intermediate event's own id when it differs from the paragraph's
    /// (`id: X`), or `Some("")` for none (`unnamed: true`).
    pub node_id: Option<String>,
    pub inputs: Vec<String>,
}

// ------------------------------------------------------------- names

pub(crate) const EVENT_TYPES: &[(EventType, &str)] = &[
    (EventType::Basic, "basic"),
    (EventType::Undeveloped, "undeveloped"),
    (EventType::External, "house"),
    (EventType::Dormant, "dormant"),
    (EventType::ConditionAnalysed, "condition"),
    (EventType::ConditionNotAnalysed, "condition_not_analysed"),
    (EventType::TransferIn, "transfer_in"),
];

const APPEARANCES: &[(EventType, &str)] = &[
    (EventType::Basic, "Basic"),
    (EventType::Undeveloped, "Undeveloped"),
    (EventType::External, "House"),
];

fn lookup(names: &[(EventType, &'static str)], s: &str) -> Option<EventType> {
    names
        .iter()
        .find(|(_, n)| n.eq_ignore_ascii_case(s))
        .map(|(k, _)| *k)
}

pub(crate) fn name_of(names: &[(EventType, &'static str)], k: EventType) -> Option<&'static str> {
    names.iter().find(|(e, _)| *e == k).map(|(_, n)| *n)
}

fn kind_list(names: &[(EventType, &str)]) -> String {
    names.iter().map(|(_, n)| *n).collect::<Vec<_>>().join(", ")
}

impl GateType {
    fn parse(s: &str) -> Option<Self> {
        let u = s.trim().to_ascii_uppercase();
        if let Some(k) = u.strip_prefix("VOTE(").and_then(|r| r.strip_suffix(')')) {
            return k.trim().parse().ok().map(GateType::Vote);
        }
        Some(match u.as_str() {
            "NULL" => GateType::Null,
            "AND" => GateType::And,
            "OR" => GateType::Or,
            "XOR" => GateType::Xor,
            "INHIBIT" => GateType::Inhibit,
            "PRIORITY_AND" => GateType::PriorityAnd,
            _ => return None,
        })
    }

    /// The name in a file (and in PFTA's tables).
    pub fn name(self) -> String {
        match self {
            GateType::Null => "NULL".into(),
            GateType::And => "AND".into(),
            GateType::Or => "OR".into(),
            GateType::Vote(k) => format!("VOTE({k})"),
            GateType::Xor => "XOR".into(),
            GateType::Inhibit => "INHIBIT".into(),
            GateType::PriorityAnd => "PRIORITY_AND".into(),
        }
    }

    /// OpenFTA's gate kind, for the types that have one.
    pub fn openfta_kind(self) -> Option<EventType> {
        match self {
            GateType::And => Some(EventType::And),
            GateType::Or => Some(EventType::Or),
            GateType::Xor => Some(EventType::Xor),
            GateType::Inhibit => Some(EventType::Inhibit),
            GateType::PriorityAnd => Some(EventType::PriorityAnd),
            GateType::Null | GateType::Vote(_) => None,
        }
    }

    pub fn from_openfta(k: EventType) -> Option<Self> {
        Some(match k {
            EventType::And => GateType::And,
            EventType::Or => GateType::Or,
            EventType::Xor => GateType::Xor,
            EventType::Inhibit => GateType::Inhibit,
            EventType::PriorityAnd => GateType::PriorityAnd,
            _ => return None,
        })
    }

    /// Is it one of PFTA's types?
    pub fn is_pfta(self) -> bool {
        matches!(
            self,
            GateType::Null | GateType::And | GateType::Or | GateType::Vote(_)
        )
    }
}

// ------------------------------------------------------------- paragraphs

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Header {
    Properties,
    Model,
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

    fn text(&mut self, key: &str) -> Result<Option<String>> {
        Ok(self.take(key)?.map(|v| v.0))
    }

    /// After all known keys were taken: anything left is an error.
    fn finish(self) -> Result<()> {
        match self.fields.into_iter().next() {
            None => Ok(()),
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
        // Every CR before the line end goes (not only one), so a value
        // written and read back is the same.
        let line = raw.trim_end_matches('\r').trim_start();
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
                        "field outside a paragraph (start one with `Gate: ID`, `Event: ID` or `Model: ID`)",
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
        let (head, id) = line.split_once(':').ok_or_else(|| {
            err(
                ln,
                "expected `Gate: ID`, `Event: ID`, `Model: ID` or `- key: value`",
            )
        })?;
        let header = match head.trim() {
            "Gate" => Header::Gate,
            "Event" => Header::Event,
            "Model" => Header::Model,
            h => {
                return Err(err(
                    ln,
                    format!("unknown paragraph `{h}` (Gate, Event or Model)"),
                ))
            }
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

/// Ids: not empty, no spaces, `,`, `;` or control characters, not `NULL`
/// (how a `.fta` file writes "no id"). PFTA allows fewer characters (see
/// [`is_pfta_id`]).
pub(crate) fn valid_id(id: &str) -> bool {
    !(id.is_empty()
        || id == "NULL"
        || id.len() > MAX_TEXT
        || id
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || c == ',' || c == ';'))
}

pub(crate) fn check_id(id: &str, line: usize) -> Result<()> {
    if !valid_id(id) {
        return Err(err(
            line,
            format!("bad id `{id}`: ids are required and cannot contain spaces, `,` or `;`"),
        ));
    }
    Ok(())
}

/// PFTA's ids: ASCII letters, digits, `_` and `-`.
pub fn is_pfta_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

// ------------------------------------------------------------- values

pub(crate) fn number(v: &str, line: usize, key: &str) -> Result<f64> {
    v.trim()
        .parse()
        .map_err(|_| err(line, format!("bad number `{v}` for `{key}`")))
}

fn integer(v: &str, line: usize, key: &str, min: usize) -> Result<usize> {
    match v.trim().parse::<usize>() {
        Ok(n) if n >= min => Ok(n),
        _ => Err(err(
            line,
            format!("`{key}` must be a whole number of at least {min}, not `{v}`"),
        )),
    }
}

fn flag(v: &str, line: usize, key: &str) -> Result<bool> {
    match v.trim() {
        "true" | "True" => Ok(true),
        "false" | "False" => Ok(false),
        _ => Err(err(line, format!("`{key}` is `True` or `False`"))),
    }
}

/// Parameter names of each distribution, in the order they are written.
const DISTRIBUTIONS: &[(&str, &[&str])] = &[
    ("beta", &["alpha", "beta"]),
    ("gamma", &["alpha", "lambda"]),
    ("lognormal", &["mu", "sigma"]),
    ("loguniform", &["lower", "upper"]),
    ("normal", &["mu", "sigma"]),
    ("triangular", &["lower", "upper", "mode"]),
    ("uniform", &["lower", "upper"]),
];

/// `0.1`, `nan`, `inf` or `name(p=..., q=...)`.
pub(crate) fn param(v: &str, line: usize, key: &str) -> Result<Param> {
    let v = v.trim();
    let Some((name, rest)) = v.split_once('(') else {
        return number(v, line, key).map(Param::Value);
    };
    let name = name.trim();
    let Some(&(_, names)) = DISTRIBUTIONS.iter().find(|(n, _)| *n == name) else {
        let known: Vec<&str> = DISTRIBUTIONS.iter().map(|d| d.0).collect();
        return Err(err(
            line,
            format!("unknown distribution `{name}` ({})", known.join(", ")),
        ));
    };
    let body = rest
        .trim_end()
        .strip_suffix(')')
        .ok_or_else(|| err(line, format!("`{v}`: missing `)`")))?;
    let mut given: BTreeMap<&str, f64> = BTreeMap::new();
    for part in body.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        let (k, x) = part
            .split_once('=')
            .ok_or_else(|| err(line, format!("`{part}`: expected `name=value`")))?;
        let k = k.trim();
        if !names.contains(&k) {
            return Err(err(
                line,
                format!("{name} has parameters {}, not `{k}`", names.join(", ")),
            ));
        }
        if given.insert(k, number(x, line, k)?).is_some() {
            return Err(err(line, format!("`{k}` is given twice")));
        }
    }
    let get = |k: &str| {
        given
            .get(k)
            .copied()
            .ok_or_else(|| err(line, format!("{name} needs `{k}`")))
    };
    let d = match name {
        "beta" => Distribution::Beta {
            alpha: get("alpha")?,
            beta: get("beta")?,
        },
        "gamma" => Distribution::Gamma {
            alpha: get("alpha")?,
            lambda: get("lambda")?,
        },
        "lognormal" => Distribution::LogNormal {
            mu: get("mu")?,
            sigma: get("sigma")?,
        },
        "loguniform" => Distribution::LogUniform {
            lower: get("lower")?,
            upper: get("upper")?,
        },
        "normal" => Distribution::Normal {
            mu: get("mu")?,
            sigma: get("sigma")?,
        },
        "triangular" => Distribution::Triangular {
            lower: get("lower")?,
            upper: get("upper")?,
            mode: get("mode")?,
        },
        _ => Distribution::Uniform {
            lower: get("lower")?,
            upper: get("upper")?,
        },
    };
    check_distribution(&d).map_err(|m| err(line, m))?;
    Ok(Param::Dist(d))
}

/// Parameters PFTA refuses: beta and gamma need finite positive ones,
/// loguniform positive bounds.
fn check_distribution(d: &Distribution) -> std::result::Result<(), String> {
    let pos = |name: &str, k: &str, x: f64| {
        if x.is_finite() && x > 0.0 {
            Ok(())
        } else {
            Err(format!("{name} parameter `{k}={x}` is not finite positive"))
        }
    };
    match *d {
        Distribution::Beta { alpha, beta } => {
            pos("beta", "alpha", alpha)?;
            pos("beta", "beta", beta)
        }
        Distribution::Gamma { alpha, lambda } => {
            pos("gamma", "alpha", alpha)?;
            pos("gamma", "lambda", lambda)
        }
        Distribution::LogUniform { lower, upper } => {
            if lower > 0.0 && upper > 0.0 {
                Ok(())
            } else {
                Err("loguniform bounds must be positive".into())
            }
        }
        _ => Ok(()),
    }
}

/// Point values PFTA would refuse when sampling: a probability outside
/// 0..1, a negative rate, intensity or mean time. `nan` passes, as there.
pub(crate) fn check_point(key: &str, p: &Param, line: usize) -> Result<()> {
    let Param::Value(x) = *p else { return Ok(()) };
    let bad = if key == "probability" {
        x < 0.0 || x > 1.0
    } else {
        x < 0.0
    };
    if bad {
        let what = if key == "probability" {
            "must be within 0..1"
        } else {
            "cannot be negative"
        };
        return Err(err(line, format!("`{key}` {what} (got {x})")));
    }
    Ok(())
}

// ------------------------------------------------------------- models

const MODEL_KEYS: &[&str] = &[
    "probability",
    "intensity",
    "failure_rate",
    "mean_failure_time",
    "repair_rate",
    "mean_repair_time",
];

/// Take the model keys of a paragraph: `model_type` and its parameters.
/// Without `model_type`, ftar's short forms: `probability` alone (a fixed
/// probability, intensity 0) or `failure_rate` alone (no repair).
fn take_model(p: &mut Paragraph) -> Result<Option<Model>> {
    let mut given: BTreeMap<&str, (Param, usize)> = BTreeMap::new();
    for key in MODEL_KEYS {
        if let Some((v, l)) = p.take(key)? {
            let x = param(&v, l, key)?;
            check_point(key, &x, l)?;
            given.insert(key, (x, l));
        }
    }
    let line = p.line;
    let Some((t, tl)) = p.take("model_type")? else {
        let keys: Vec<&str> = given.keys().copied().collect();
        return match keys.as_slice() {
            [] => Ok(None),
            ["probability"] => Ok(given.get("probability").map(|(x, _)| Model::Fixed {
                probability: *x,
                intensity: Param::Value(0.0),
            })),
            ["failure_rate"] => Ok(given.get("failure_rate").map(|(x, _)| Model::ConstantRate {
                failure: Rate::Rate(*x),
                repair: Rate::Rate(Param::Value(0.0)),
            })),
            _ => Err(err(
                line,
                format!(
                    "{} need a `model_type` (Fixed, ConstantRate, True or False)",
                    keys.join(", ")
                ),
            )),
        };
    };
    let take = |k: &str| given.get(k).map(|x| x.0);
    let combo = |want: &[&str]| -> Result<()> {
        let keys: HashSet<&str> = given.keys().copied().collect();
        if keys == want.iter().copied().collect() {
            Ok(())
        } else {
            let mut have: Vec<&str> = keys.into_iter().collect();
            have.sort_unstable();
            Err(err(
                tl,
                format!(
                    "model type `{t}` takes {}, not {{{}}}",
                    if want.is_empty() {
                        "no parameters".to_string()
                    } else {
                        want.join(" and ")
                    },
                    have.join(", ")
                ),
            ))
        }
    };
    let rate = |r: &'static str, mean: &'static str| match (take(r), take(mean)) {
        (Some(x), None) => Ok((Rate::Rate(x), r)),
        (None, Some(x)) => Ok((Rate::MeanTime(x), mean)),
        _ => Err(err(tl, format!("model type `{t}` takes `{r}` or `{mean}`"))),
    };
    Ok(Some(match t.trim() {
        "Fixed" => {
            combo(&["probability", "intensity"])?;
            Model::Fixed {
                probability: take("probability").unwrap_or(Param::Value(0.0)),
                intensity: take("intensity").unwrap_or(Param::Value(0.0)),
            }
        }
        "ConstantRate" => {
            let (failure, k1) = rate("failure_rate", "mean_failure_time")?;
            let (repair, k2) = rate("repair_rate", "mean_repair_time")?;
            combo(&[k1, k2])?;
            Model::ConstantRate { failure, repair }
        }
        "True" => {
            combo(&[])?;
            Model::True
        }
        "False" => {
            combo(&[])?;
            Model::False
        }
        _ => {
            return Err(err(
                tl,
                format!("unknown model type `{t}` (Fixed, ConstantRate, True or False)"),
            ))
        }
    }))
}

// ------------------------------------------------------------- paragraphs -> Ast

fn props(mut p: Paragraph) -> Result<Props> {
    let note = p.fields.remove("note").map(|(v, _)| v.join("\n"));
    let mut out = Props {
        note,
        ..Props::default()
    };
    if let Some((t, l)) = p.take("transfer_out")? {
        check_id(&t, l)?;
        out.transfer_out = Some(t);
    }
    if let Some((v, l)) = p.take("times")? {
        let mut times = Vec::new();
        for s in v.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            let t = number(s, l, "times")?;
            if t < 0.0 {
                return Err(err(l, format!("negative time `{s}`")));
            }
            times.push(t);
        }
        out.times = Some(times);
    }
    out.time_unit = p.text("time_unit")?;
    out.seed = p.text("seed")?;
    if let Some((v, l)) = p.take("sample_size")? {
        out.sample_size = Some(integer(&v, l, "sample_size", 1)?);
    }
    if let Some((v, l)) = p.take("computational_order")? {
        out.computational_order = Some(integer(&v, l, "computational_order", 1)?);
    }
    if let Some((v, l)) = p.take("computational_tolerance")? {
        let x = number(&v, l, "computational_tolerance")?;
        if !(0.0..1.0).contains(&x) {
            return Err(err(l, "`computational_tolerance` must be within 0..1"));
        }
        out.computational_tolerance = Some(x);
    }
    if let Some((v, l)) = p.take("significant_figures")? {
        let n = integer(&v, l, "significant_figures", 1)?;
        out.significant_figures = Some(u32::try_from(n).map_err(|_| err(l, "too large"))?);
    }
    if let Some((v, l)) = p.take("scientific_exponent")? {
        let n = integer(&v, l, "scientific_exponent", 0)?;
        out.scientific_exponent = Some(u32::try_from(n).map_err(|_| err(l, "too large"))?);
    }
    p.finish()?;
    Ok(out)
}

fn model_def(mut p: Paragraph) -> Result<ModelDef> {
    let label = p.text("label")?;
    let comment = p.text("comment")?;
    let model = take_model(&mut p)?
        .ok_or_else(|| err(p.line, format!("model `{}` needs a `model_type`", p.id)))?;
    let (id, line) = (p.id.clone(), p.line);
    p.finish()?;
    Ok(ModelDef {
        id,
        line,
        label,
        comment,
        model,
    })
}

fn event_def(mut p: Paragraph) -> Result<EventDef> {
    let label = p.text("label")?;
    let comment = p.text("comment")?;
    let kind = match (p.take("type")?, p.take("appearance")?) {
        (Some(_), Some((_, l))) => return Err(err(l, "give `type` or `appearance`, not both")),
        (Some((t, l)), None) => lookup(EVENT_TYPES, &t).ok_or_else(|| {
            err(
                l,
                format!("unknown event type `{t}` ({})", kind_list(EVENT_TYPES)),
            )
        })?,
        (None, Some((t, l))) => lookup(APPEARANCES, &t).ok_or_else(|| {
            err(
                l,
                format!("unknown appearance `{t}` ({})", kind_list(APPEARANCES)),
            )
        })?,
        (None, None) => EventType::Basic,
    };
    let dormant = match p.take("dormant")? {
        Some((v, l)) => flag(&v, l, "dormant")?,
        None => false,
    };
    let group = p.text("group")?;
    let own = take_model(&mut p)?;
    let model = match (p.take("model")?, own) {
        (Some((_, l)), Some(_)) => {
            return Err(err(
                l,
                "give `model` or the event's own model keys, not both",
            ))
        }
        (Some((m, l)), None) => {
            check_id(&m, l)?;
            Some(EventModel::Shared(m))
        }
        (None, own) => own.map(EventModel::Own),
    };
    if kind == EventType::TransferIn && model.is_some() {
        return Err(err(
            p.line,
            "a transfer-in has no model: its tree's events have them",
        ));
    }
    let (id, line) = (p.id.clone(), p.line);
    p.finish()?;
    Ok(EventDef {
        id,
        line,
        label,
        comment,
        kind,
        dormant,
        group,
        model,
    })
}

fn gate_def(mut p: Paragraph) -> Result<GateDef> {
    let label = p.text("label")?;
    let comment = p.text("comment")?;
    let is_paged = match p.take("is_paged")? {
        Some((v, l)) => flag(&v, l, "is_paged")?,
        None => false,
    };
    let node_id = match (p.take("id")?, p.take("unnamed")?) {
        (Some(_), Some((_, l))) => return Err(err(l, "give `id` or `unnamed`, not both")),
        (Some((v, l)), None) => {
            check_id(&v, l)?;
            Some(v)
        }
        (None, Some((v, l))) => flag(&v, l, "unnamed")?.then(String::new),
        (None, None) => None,
    };
    let (gate_type, type_line) = match p.take("type")? {
        None => (GateType::Null, None),
        Some((t, l)) => (
            GateType::parse(&t).ok_or_else(|| {
                err(
                    l,
                    format!("unknown gate type `{t}` (AND, OR, VOTE(k), NULL, XOR, INHIBIT or PRIORITY_AND)"),
                )
            })?,
            Some(l),
        ),
    };
    let gate_id = match p.take("gate_id")? {
        Some((_, l)) if gate_type.openfta_kind().is_none() => {
            return Err(err(
                l,
                "`gate_id` needs a `type` of OpenFTA's (AND, OR, XOR, INHIBIT, PRIORITY_AND)",
            ))
        }
        Some((g, l)) => {
            check_id(&g, l)?;
            Some(g)
        }
        None => None,
    };
    let mut inputs = Vec::new();
    if let Some((list, l)) = p.take("inputs")? {
        // PFTA allows one trailing comma.
        let list = list.trim_end();
        let list = list.strip_suffix(',').unwrap_or(list);
        if !list.trim().is_empty() {
            for s in list.split(',').map(str::trim) {
                check_id(s, l)?;
                inputs.push(s.to_string());
            }
        }
    }
    if let Some(l) = type_line {
        if gate_type == GateType::Null && inputs.len() != 1 {
            return Err(err(l, "a NULL gate has exactly one input"));
        }
    }
    let (id, line) = (p.id.clone(), p.line);
    p.finish()?;
    Ok(GateDef {
        id,
        line,
        label,
        comment,
        is_paged,
        gate_type,
        gate_id,
        node_id,
        inputs,
    })
}

/// Read a file into paragraphs, checking every key and value. Ids must be
/// unique across models, events and gates.
pub fn parse_ast(text: &str) -> Result<Ast> {
    let mut ast = Ast::default();
    let mut seen: HashSet<String> = HashSet::new();
    for p in paragraphs(text)? {
        if p.header == Header::Properties {
            ast.props = props(p)?;
            continue;
        }
        check_id(&p.id, p.line)?;
        if !seen.insert(p.id.clone()) {
            return Err(err(p.line, format!("`{}` is defined twice", p.id)));
        }
        match p.header {
            Header::Model => ast.models.push(model_def(p)?),
            Header::Event => ast.events.push(event_def(p)?),
            _ => ast.gates.push(gate_def(p)?),
        }
    }
    Ok(ast)
}

// ------------------------------------------------------------- rendering

/// Shortest exact form, in exponent notation for very small or large values.
pub(crate) fn format_number(x: f64) -> String {
    if x.is_nan() {
        return "nan".into();
    }
    if x.is_infinite() {
        return if x.is_sign_positive() { "inf" } else { "-inf" }.into();
    }
    let a = x.abs();
    if x == 0.0 || (1e-4..1e6).contains(&a) {
        format!("{x}")
    } else {
        format!("{x:e}")
    }
}

fn render_param(p: &Param) -> String {
    let Param::Dist(d) = p else {
        let Param::Value(x) = p else {
            return String::new();
        };
        return format_number(*x);
    };
    let f = format_number;
    match *d {
        Distribution::Beta { alpha, beta } => format!("beta(alpha={}, beta={})", f(alpha), f(beta)),
        Distribution::Gamma { alpha, lambda } => {
            format!("gamma(alpha={}, lambda={})", f(alpha), f(lambda))
        }
        Distribution::LogNormal { mu, sigma } => {
            format!("lognormal(mu={}, sigma={})", f(mu), f(sigma))
        }
        Distribution::LogUniform { lower, upper } => {
            format!("loguniform(lower={}, upper={})", f(lower), f(upper))
        }
        Distribution::Normal { mu, sigma } => format!("normal(mu={}, sigma={})", f(mu), f(sigma)),
        Distribution::Triangular { lower, upper, mode } => format!(
            "triangular(lower={}, upper={}, mode={})",
            f(lower),
            f(upper),
            f(mode)
        ),
        Distribution::Uniform { lower, upper } => {
            format!("uniform(lower={}, upper={})", f(lower), f(upper))
        }
    }
}

struct Out(String);

impl Out {
    fn line(&mut self, key: &str, value: &str) {
        self.0.push_str("- ");
        self.0.push_str(key);
        self.0.push(':');
        if !value.is_empty() {
            self.0.push(' ');
            self.0.push_str(value);
        }
        self.0.push('\n');
    }

    fn opt(&mut self, key: &str, value: &Option<String>) {
        if let Some(v) = value {
            self.line(key, v);
        }
    }

    fn header(&mut self, class: &str, id: &str) {
        self.0.push('\n');
        self.0.push_str(class);
        self.0.push_str(": ");
        self.0.push_str(id);
        self.0.push('\n');
    }

    fn model(&mut self, m: &Model) {
        match m {
            Model::Fixed {
                probability,
                intensity,
            } => {
                self.line("model_type", "Fixed");
                self.line("probability", &render_param(probability));
                self.line("intensity", &render_param(intensity));
            }
            Model::ConstantRate { failure, repair } => {
                self.line("model_type", "ConstantRate");
                for (r, rate, mean) in [
                    (failure, "failure_rate", "mean_failure_time"),
                    (repair, "repair_rate", "mean_repair_time"),
                ] {
                    match r {
                        Rate::Rate(x) => self.line(rate, &render_param(x)),
                        Rate::MeanTime(x) => self.line(mean, &render_param(x)),
                    }
                }
            }
            Model::True => self.line("model_type", "True"),
            Model::False => self.line("model_type", "False"),
        }
    }
}

fn render_props(o: &mut Out, p: &Props) {
    if let Some(t) = &p.times {
        let s: Vec<String> = t.iter().map(|x| format_number(*x)).collect();
        o.line("times", &s.join(", "));
    }
    o.opt("time_unit", &p.time_unit);
    o.opt("seed", &p.seed);
    let num = |x: Option<usize>| x.map(|n| n.to_string());
    o.opt("sample_size", &num(p.sample_size));
    o.opt("computational_order", &num(p.computational_order));
    o.opt(
        "computational_tolerance",
        &p.computational_tolerance.map(format_number),
    );
    o.opt(
        "significant_figures",
        &p.significant_figures.map(|n| n.to_string()),
    );
    o.opt(
        "scientific_exponent",
        &p.scientific_exponent.map(|n| n.to_string()),
    );
    o.opt("transfer_out", &p.transfer_out);
    if let Some(note) = &p.note {
        for l in note.split('\n') {
            o.line("note", l.trim_end_matches('\r'));
        }
    }
}

/// Write paragraphs as text: properties, models, gates, then events. Keys
/// PFTA knows come first in each paragraph, ftar's own after them.
pub fn render(ast: &Ast) -> String {
    let mut o = Out(String::from(
        "# ftar fault tree, text format (docs/TEXT-FORMAT.md)\n",
    ));
    render_props(&mut o, &ast.props);
    for m in &ast.models {
        o.header("Model", &m.id);
        o.opt("label", &m.label);
        o.opt("comment", &m.comment);
        o.model(&m.model);
    }
    for g in &ast.gates {
        o.header("Gate", &g.id);
        o.opt("label", &g.label);
        o.opt("comment", &g.comment);
        if g.is_paged {
            o.line("is_paged", "True");
        }
        if g.gate_type != GateType::Null || g.inputs.len() == 1 {
            o.line("type", &g.gate_type.name());
        }
        o.line("inputs", &g.inputs.join(", "));
        o.opt("gate_id", &g.gate_id);
        match g.node_id.as_deref() {
            Some("") => o.line("unnamed", "True"),
            Some(id) => o.line("id", id),
            None => {}
        }
    }
    for e in &ast.events {
        o.header("Event", &e.id);
        o.opt("label", &e.label);
        o.opt("comment", &e.comment);
        if let Some(a) = name_of(APPEARANCES, e.kind) {
            if e.kind != EventType::Basic {
                o.line("appearance", a);
            }
        } else if let Some(t) = name_of(EVENT_TYPES, e.kind) {
            o.line("type", t);
        }
        match &e.model {
            Some(EventModel::Own(m)) => o.model(m),
            Some(EventModel::Shared(id)) => o.line("model", id),
            None => {}
        }
        o.opt("group", &e.group);
        if e.dormant {
            o.line("dormant", "True");
        }
    }
    o.0
}
