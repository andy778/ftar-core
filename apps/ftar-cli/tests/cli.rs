//! Black-box tests of the `ftar` binary (what agents and CI scripts see).

use std::process::Command;

const DATA: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../crates/core/tests/data");

fn ftar(args: &[&str]) -> (bool, String) {
    let o = Command::new(env!("CARGO_BIN_EXE_ftar"))
        .args(args)
        .output()
        .expect("run ftar");
    (
        o.status.success(),
        String::from_utf8_lossy(&o.stdout).into_owned(),
    )
}

#[test]
fn prob_prints_exact_value() {
    let (ok, out) = ftar(&["prob", &format!("{DATA}/theatre.fta")]);
    assert!(ok);
    assert!(out.contains("Exact value : 2.0643378107574753E-3"), "{out}");
}

#[test]
fn tools_lists_json_catalogue() {
    let (ok, out) = ftar(&["tools"]);
    assert!(ok);
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert!(v
        .as_array()
        .unwrap()
        .iter()
        .any(|t| t["name"] == "analysis_cutsets"));
}

#[test]
fn call_returns_json_and_failure_exit_code() {
    let (ok, out) = ftar(&[
        "call",
        "tree_open",
        r#"{"path":"theatre.fta"}"#,
        "--root",
        DATA,
    ]);
    assert!(ok, "{out}");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&out).unwrap()["primary_events"],
        3
    );
    let (ok, out) = ftar(&[
        "call",
        "tree_open",
        r#"{"path":"../x.fta"}"#,
        "--root",
        DATA,
    ]);
    assert!(!ok);
    assert!(out.contains("confinement"), "{out}");
}

#[test]
fn run_script_file() {
    let dir = std::env::temp_dir().join(format!("ftar_cli_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let script = dir.join("s.json");
    std::fs::write(
        &script,
        r#"{"steps":[{"tool":"tree_open","args":{"path":"lift.fta"}},{"tool":"analysis_cutsets"}]}"#,
    )
    .unwrap();
    let (ok, out) = ftar(&["run", script.to_str().unwrap(), "--root", DATA]);
    assert!(ok, "{out}");
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v[1]["result"]["count"], 12);
    let _ = std::fs::remove_dir_all(dir);
}
