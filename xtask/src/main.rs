//! Build tooling: `cargo xtask <command>`. Rust only, no dependencies.
//!
//! * `ci`      – every gate below, in order (what CI and agents run before a commit)
//! * `fmt`     – `cargo fmt --check`
//! * `clippy`  – clippy with `-D warnings` on all targets
//! * `layers`  – architecture rules (see docs/architecture.md)
//! * `test`    – unit, integration and headless GUI tests, then doctests
//! * `deny`    – dependency bans, licences and sources (deny.toml)
//!
//! Not part of `ci` (slower; see CONTRIBUTING.md):
//! * `mutants [BASE]` – mutation-test the Rust lines changed since BASE (default `origin/master`),
//!   committed or not, to check the tests catch bugs there (needs cargo-mutants)
//! * `coverage`       – line coverage of the workspace tests as HTML (needs cargo-llvm-cov)

use std::path::PathBuf;
use std::process::{Command, ExitCode, Stdio};

mod layers;

/// The gates `cargo xtask ci` runs, in order: cheap checks first.
const CI_STEPS: &[&str] = &["fmt", "clippy", "layers", "test", "deny"];

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let cmd = args.next().unwrap_or_else(|| "ci".into());
    let result = match cmd.as_str() {
        "ci" => ci(),
        "mutants" => mutants(args.next().as_deref().unwrap_or("origin/master")),
        "coverage" => coverage(),
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
            "unknown command `{cmd}` (try: ci, fmt, clippy, layers, test, deny, mutants, coverage)"
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

/// Mutation testing of what this branch changed: the Rust diff from the merge base with `base`
/// to the working tree (so uncommitted edits count; a new file only once it is tracked). Every mutant must be caught by a test; a missed one is printed with its line.
fn mutants(base: &str) -> Result<(), String> {
    if !installed(&["mutants", "--version"]) {
        return Err("cargo-mutants is not installed (cargo install cargo-mutants --locked)".into());
    }
    let merge_base = git(&["merge-base", base, "HEAD"])?;
    let diff = git(&["diff", merge_base.trim(), "--", "*.rs"])?;
    if diff.trim().is_empty() {
        println!("mutants: no Rust changes since {base}, nothing to test");
        return Ok(());
    }
    let dir = target_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let file = dir.join("mutants.diff");
    std::fs::write(&file, diff).map_err(|e| format!("{}: {e}", file.display()))?;
    // cargo-mutants builds a copy of the tree. With a shared CARGO_TARGET_DIR its mutated builds
    // would land in (and fight over) the normal target dir, and leave test binaries there whose
    // compiled-in CARGO_MANIFEST_DIR points at the deleted copy; give it its own.
    let status = cargo_cmd()
        .args(["mutants", "--in-diff"])
        .arg(&file)
        .env_remove("CARGO_TARGET_DIR")
        .status()
        .map_err(|e| e.to_string())?;
    status.success().then_some(()).ok_or_else(|| {
        "cargo mutants found mutants the tests miss (see mutants.out/missed.txt)".into()
    })
}

/// Line coverage of the workspace tests (nextest when installed), as an HTML report and a
/// per-file summary. Informational: there is no threshold to pass.
fn coverage() -> Result<(), String> {
    if !installed(&["llvm-cov", "--version"]) {
        return Err(
            "cargo-llvm-cov is not installed (cargo install cargo-llvm-cov --locked;              rustup component add llvm-tools-preview)"
                .into(),
        );
    }
    let runner: &[&str] = if installed(&["nextest", "--version"]) {
        &["llvm-cov", "nextest"]
    } else {
        &["llvm-cov"]
    };
    cargo(
        &[
            runner,
            &["--workspace", "--exclude", "xtask", "--no-report"],
        ]
        .concat(),
    )?;
    cargo(&["llvm-cov", "report", "--summary-only"])?;
    cargo(&["llvm-cov", "report", "--html"])?;
    println!(
        "coverage: open {}",
        target_dir().join("llvm-cov/html/index.html").display()
    );
    Ok(())
}

/// Cargo's target directory: `CARGO_TARGET_DIR` when set (parallel agents use their own).
fn target_dir() -> PathBuf {
    std::env::var_os("CARGO_TARGET_DIR").map_or_else(|| PathBuf::from("target"), PathBuf::from)
}

/// Runs git and returns its stdout.
fn git(args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .args(args)
        .output()
        .map_err(|e| format!("git: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    String::from_utf8(out.stdout).map_err(|e| format!("git {}: {e}", args.join(" ")))
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
