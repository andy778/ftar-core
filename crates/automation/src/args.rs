//! Small typed accessors over a JSON argument object.

use ftar_core::EventType;
use serde_json::Value;

use crate::session::ToolError;

pub struct Args<'a>(pub &'a Value);

impl<'a> Args<'a> {
    pub fn str(&self, k: &str) -> Result<&'a str, ToolError> {
        self.opt_str(k)?
            .ok_or_else(|| ToolError::args(format!("missing string `{k}`")))
    }
    pub fn opt_str(&self, k: &str) -> Result<Option<&'a str>, ToolError> {
        match self.0.get(k) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(s)) => Ok(Some(s)),
            Some(_) => Err(ToolError::args(format!("`{k}` must be a string"))),
        }
    }
    pub fn opt_u64(&self, k: &str) -> Result<Option<u64>, ToolError> {
        match self.0.get(k) {
            None | Some(Value::Null) => Ok(None),
            Some(v) => v
                .as_u64()
                .map(Some)
                .ok_or_else(|| ToolError::args(format!("`{k}` must be a non-negative integer"))),
        }
    }
    pub fn opt_usize(&self, k: &str) -> Result<Option<usize>, ToolError> {
        Ok(self
            .opt_u64(k)?
            .map(|v| usize::try_from(v).unwrap_or(usize::MAX)))
    }
    pub fn opt_f64(&self, k: &str) -> Result<Option<f64>, ToolError> {
        match self.0.get(k) {
            None | Some(Value::Null) => Ok(None),
            Some(v) => v
                .as_f64()
                .filter(|x| x.is_finite())
                .map(Some)
                .ok_or_else(|| ToolError::args(format!("`{k}` must be a finite number"))),
        }
    }
    pub fn opt_bool(&self, k: &str) -> Result<Option<bool>, ToolError> {
        match self.0.get(k) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::Bool(b)) => Ok(Some(*b)),
            Some(_) => Err(ToolError::args(format!("`{k}` must be a boolean"))),
        }
    }
    /// A node path: array of child indices.
    pub fn path(&self, k: &str) -> Result<Vec<usize>, ToolError> {
        self.opt_path(k)?
            .ok_or_else(|| ToolError::args(format!("missing path `{k}`")))
    }
    pub fn opt_path(&self, k: &str) -> Result<Option<Vec<usize>>, ToolError> {
        match self.0.get(k) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::Array(a)) => a
                .iter()
                .map(|v| {
                    v.as_u64()
                        .and_then(|n| usize::try_from(n).ok())
                        .ok_or_else(|| {
                            ToolError::args(format!("`{k}` must be an array of indices"))
                        })
                })
                .collect::<Result<Vec<_>, _>>()
                .map(Some),
            Some(_) => Err(ToolError::args(format!(
                "`{k}` must be an array of indices"
            ))),
        }
    }
    pub fn kind(&self, k: &str) -> Result<EventType, ToolError> {
        parse_kind(self.str(k)?)
    }
}

/// Accepts the one-letter file code (`"A"`) or a name (`"and"`, `"basic"`, ...).
pub fn parse_kind(s: &str) -> Result<EventType, ToolError> {
    use EventType::*;
    let by_name = match s.to_ascii_lowercase().as_str() {
        "and" => Some(And),
        "or" => Some(Or),
        "xor" => Some(Xor),
        "priority_and" | "pand" => Some(PriorityAnd),
        "inhibit" => Some(Inhibit),
        "basic" => Some(Basic),
        "undeveloped" => Some(Undeveloped),
        "external" => Some(External),
        "dormant" => Some(Dormant),
        "intermediate" => Some(Intermediate),
        "transfer_in" => Some(TransferIn),
        "transfer_out" => Some(TransferOut),
        "condition" | "condition_analysed" => Some(ConditionAnalysed),
        "condition_not_analysed" => Some(ConditionNotAnalysed),
        _ => None,
    };
    if let Some(k) = by_name {
        return Ok(k);
    }
    let mut it = s.chars();
    match (it.next(), it.next()) {
        (Some(c), None) => EventType::from_code(c),
        _ => None,
    }
    .ok_or_else(|| ToolError::args(format!("unknown event kind `{s}`")))
}

pub fn kind_name(k: EventType) -> &'static str {
    use EventType::*;
    match k {
        Unknown => "unknown",
        Basic => "basic",
        And => "and",
        Or => "or",
        TransferIn => "transfer_in",
        TransferOut => "transfer_out",
        Intermediate => "intermediate",
        Undeveloped => "undeveloped",
        Inhibit => "inhibit",
        PriorityAnd => "priority_and",
        Xor => "xor",
        External => "external",
        ConditionNotAnalysed => "condition_not_analysed",
        ConditionAnalysed => "condition_analysed",
        Dormant => "dormant",
    }
}
