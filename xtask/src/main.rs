//! Build tooling: `cargo xtask <command>`. Rust only, no dependencies.
//!
//! * `ci`      – every gate below, in order (what CI and agents run before a commit)
//! * `fmt`     – `cargo fmt --check`
//! * `clippy`  – clippy with `-D warnings` on all targets
//! * `test`    – unit, integration and headless GUI tests
//! * `layers`  – architecture rules (see docs/architecture.md)
//! * `assets`  – no binary assets without an entry in ATTRIBUTION.toml

use std::process::{Command, ExitCode};

mod layers;

fn main() -> ExitCode {
    let cmd = std::env::args().nth(1).unwrap_or_else(|| "ci".into());
    let result = match cmd.as_str() {
        "ci" => ["fmt", "clippy", "layers", "test"]
            .iter()
            .try_for_each(|c| run(c)),
        other => run(other),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("xtask: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(cmd: &str) -> Result<(), String> {
    eprintln!("==> {cmd}");
    match cmd {
        "fmt" => cargo(&["fmt", "--all", "--check"]),
        "clippy" => cargo(&[
            "clippy",
            "--workspace",
            "--all-targets",
            "--",
            "-D",
            "warnings",
        ]),
        "test" => cargo(&["test", "--workspace"]),
        "layers" => layers::check(),
        _ => Err(format!(
            "unknown command `{cmd}` (try: ci, fmt, clippy, test, layers)"
        )),
    }
}

fn cargo(args: &[&str]) -> Result<(), String> {
    let status = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .args(args)
        .status()
        .map_err(|e| e.to_string())?;
    status
        .success()
        .then_some(())
        .ok_or_else(|| format!("cargo {} failed", args.join(" ")))
}
