//! Build tooling: `cargo xtask <command>`. Rust only, no dependencies.
//!
//! * `ci`      – every gate below, in order (what CI and agents run before a commit)
//! * `fmt`     – `cargo fmt --check`
//! * `clippy`  – clippy with `-D warnings` on all targets
//! * `layers`  – architecture rules (see docs/architecture.md)
//! * `test`    – unit, integration and headless GUI tests, then doctests
//! * `deny`    – dependency bans, licences and sources (deny.toml)

use std::process::{Command, ExitCode, Stdio};

mod layers;

/// The gates `cargo xtask ci` runs, in order: cheap checks first.
const CI_STEPS: &[&str] = &["fmt", "clippy", "layers", "test", "deny"];

fn main() -> ExitCode {
    let cmd = std::env::args().nth(1).unwrap_or_else(|| "ci".into());
    let result = match cmd.as_str() {
        "ci" => ci(),
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

/// Runs every gate and ends with a one-line summary naming the step that failed, so a log
/// reader (human or agent) does not have to scroll back to find it.
fn ci() -> Result<(), String> {
    for (passed, step) in CI_STEPS.iter().enumerate() {
        if let Err(e) = run(step) {
            println!(
                "\nci: {passed} of {} steps passed, FAILED at `{step}`",
                CI_STEPS.len()
            );
            return Err(e);
        }
    }
    println!("\nci: all {} steps passed", CI_STEPS.len());
    Ok(())
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
        "test" => test(),
        "layers" => layers::check(),
        "deny" => deny(),
        _ => Err(format!(
            "unknown command `{cmd}` (try: ci, fmt, clippy, layers, test, deny)"
        )),
    }
}

/// Tests run under cargo-nextest when it is installed (one process per test, and every failure
/// reported in one run), else under `cargo test`. Nextest does not run doctests, so they run
/// separately. Both keep going past the first failing test.
fn test() -> Result<(), String> {
    if installed(&["nextest", "--version"]) {
        cargo(&["nextest", "run", "--workspace", "--no-fail-fast"])?;
        cargo(&["test", "--workspace", "--doc"])
    } else {
        eprintln!(
            "test: cargo-nextest is not installed, using cargo test \
             (cargo install cargo-nextest --locked)"
        );
        cargo(&["test", "--workspace", "--no-fail-fast"])
    }
}

/// Bans, licences and sources from deny.toml. Advisories are left to the scheduled
/// `cargo-deny` workflow: a new RustSec entry should not fail an unrelated change. Skipped with
/// a hint when cargo-deny is not installed, except under CI (`CI` set), where it is required.
fn deny() -> Result<(), String> {
    if !installed(&["deny", "--version"]) {
        if std::env::var_os("CI").is_some() {
            return Err("cargo-deny is required in CI".into());
        }
        eprintln!("deny: SKIPPED, cargo-deny is not installed (cargo install cargo-deny --locked)");
        return Ok(());
    }
    cargo(&[
        "deny",
        "--log-level",
        "error",
        "check",
        "bans",
        "licenses",
        "sources",
    ])
}

/// Whether `cargo <args>` (a cargo subcommand such as nextest) runs successfully.
fn installed(args: &[&str]) -> bool {
    cargo_cmd()
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

fn cargo_cmd() -> Command {
    Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
}

fn cargo(args: &[&str]) -> Result<(), String> {
    let status = cargo_cmd().args(args).status().map_err(|e| e.to_string())?;
    status
        .success()
        .then_some(())
        .ok_or_else(|| format!("cargo {} failed", args.join(" ")))
}
