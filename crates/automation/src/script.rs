//! Run a JSON script: `{ "steps": [ { "tool": "...", "args": {...} }, ... ] }`.

use serde_json::{json, Value};

use crate::session::{Session, ToolError};

#[derive(Debug, Clone, PartialEq)]
pub struct StepResult {
    pub step: usize,
    pub tool: String,
    pub outcome: Result<Value, ToolError>,
}

impl StepResult {
    pub fn to_json(&self) -> Value {
        match &self.outcome {
            Ok(v) => json!({ "step": self.step, "tool": self.tool, "ok": true, "result": v }),
            Err(e) => {
                json!({ "step": self.step, "tool": self.tool, "ok": false, "error": e.to_json() })
            }
        }
    }
}

/// Run all steps in order, stopping at the first failure unless the script
/// sets `"continue_on_error": true`. A malformed script is a single failed step 0.
pub fn run_script(session: &mut Session, script: &Value) -> Vec<StepResult> {
    let Some(steps) = script.get("steps").and_then(Value::as_array) else {
        return vec![StepResult {
            step: 0,
            tool: String::new(),
            outcome: Err(ToolError::args("script needs a `steps` array")),
        }];
    };
    let keep_going = script
        .get("continue_on_error")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut out = Vec::new();
    for (i, s) in steps.iter().enumerate() {
        let tool = s
            .get("tool")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let outcome = if tool.is_empty() {
            Err(ToolError::args("step needs a `tool` name"))
        } else {
            session.call(&tool, s.get("args").unwrap_or(&Value::Null))
        };
        let failed = outcome.is_err();
        out.push(StepResult {
            step: i + 1,
            tool,
            outcome,
        });
        if failed && !keep_going {
            break;
        }
    }
    out
}
