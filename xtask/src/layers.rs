//! Enforces the layering in docs/architecture.md using `cargo tree`.
//!
//! Rule: library crates under `crates/` must not depend (even transitively)
//! on any GUI/window/dialog crate, so the whole engine stays headless-testable.

use std::process::Command;

const GUI_CRATES: &[&str] = &["egui", "eframe", "winit", "rfd", "egui_kittest"];
/// Library crates that must stay headless. Add new `crates/*` members here.
const HEADLESS: &[&str] = &["ftar-core", "ftar-automation", "ftar-mcp"];

pub fn check() -> Result<(), String> {
    let mut bad = Vec::new();
    for krate in HEADLESS {
        let out = Command::new("cargo")
            .args(["tree", "-p", krate, "--prefix", "none", "--edges", "normal"])
            .output()
            .map_err(|e| e.to_string())?;
        if !out.status.success() {
            return Err(format!("cargo tree -p {krate} failed"));
        }
        let text = String::from_utf8_lossy(&out.stdout);
        for line in text.lines() {
            let name = line.split_whitespace().next().unwrap_or("");
            if GUI_CRATES.contains(&name) {
                bad.push(format!("{krate} depends on {name}"));
            }
        }
    }
    if bad.is_empty() {
        println!("layers: ok");
        Ok(())
    } else {
        Err(bad.join("\n"))
    }
}
