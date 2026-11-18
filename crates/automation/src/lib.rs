//! `ftar-automation` - the headless JSON tool API over `ftar-core`.
//!
//! Every capability of ftar is a *tool*: a name, a JSON-Schema for its
//! arguments and a JSON result. Scripts, agents, the CLI (`ftar run`, `ftar
//! tools`) and tests all go through [`Session::call`]. Nothing here depends on
//! a GUI.
//!
//! * file access is confined to an optional `root` directory
//! * results are deterministic (Monte Carlo takes a `seed`; reports take `time`)
//! * errors are structured: `{ "code": "...", "message": "..." }`

mod args;
mod catalogue;
mod script;
mod session;
mod tools;

pub use catalogue::{tools, ToolInfo};
pub use script::{run_script, StepResult};
pub use session::{Session, ToolError};
