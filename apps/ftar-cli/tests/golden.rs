//! Golden output of the analysis commands: stdout, stderr and exit code of each case are
//! compared with `tests/golden/<case>.txt`. When a change to the output is intended,
//! regenerate with `FTAR_UPDATE_GOLDEN=1 cargo test -p ftar-cli --test golden` and review the
//! diff.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::process::Command;

/// (golden file name, arguments). Paths are relative to the workspace root.
const CASES: &[(&str, &[&str])] = &[
    ("info", &["info", "crates/core/tests/data/theatre.fta"]),
    (
        "info_note",
        &["info", "crates/core/tests/data/wrap/wrap.fta"],
    ),
    (
        "cutsets",
        &["cutsets", "crates/core/tests/data/theatre.fta"],
    ),
    (
        "cutsets_order",
        &["cutsets", "crates/core/tests/data/lift.fta", "--order", "2"],
    ),
    ("prob", &["prob", "crates/core/tests/data/theatre.fta"]),
    (
        "prob_terms_time",
        &[
            "prob",
            "crates/core/tests/data/lift.fta",
            "--terms",
            "3",
            "--time",
            "2.5",
        ],
    ),
    (
        "mc_first",
        &[
            "mc",
            "crates/core/tests/data/theatre.fta",
            "--trials",
            "2000",
            "--method",
            "first",
            "--seed",
            "7",
        ],
    ),
    (
        "mc_first_failure",
        &[
            "mc",
            "crates/core/tests/data/theatre.fta",
            "--trials",
            "2000",
            "--method",
            "first_failure",
            "--seed",
            "7",
        ],
    ),
    (
        "mc_direct",
        &[
            "mc",
            "crates/core/tests/data/theatre.fta",
            "--trials",
            "2000",
            "--method",
            "direct",
            "--seed",
            "3",
        ],
    ),
    (
        "err_bad_method",
        &[
            "mc",
            "crates/core/tests/data/theatre.fta",
            "--method",
            "bogus",
        ],
    ),
    (
        "err_missing_value",
        &["prob", "crates/core/tests/data/theatre.fta", "--terms"],
    ),
    (
        "err_unknown_option",
        &["prob", "crates/core/tests/data/theatre.fta", "--bogus", "1"],
    ),
    (
        "err_bad_number",
        &["prob", "crates/core/tests/data/theatre.fta", "--terms", "x"],
    ),
    (
        "err_unknown_command",
        &["nonsense", "crates/core/tests/data/theatre.fta"],
    ),
];

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

fn run(args: &[&str]) -> String {
    let o = Command::new(env!("CARGO_BIN_EXE_ftar"))
        .args(args)
        .current_dir(workspace())
        .output()
        .expect("run ftar");
    format!(
        "exit {}\n--- stdout\n{}--- stderr\n{}",
        o.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

#[test]
fn analysis_commands_match_golden_output() {
    let update = std::env::var_os("FTAR_UPDATE_GOLDEN").is_some();
    let mut failed = Vec::new();
    for (name, args) in CASES {
        let actual = run(args);
        let path = golden_dir().join(format!("{name}.txt"));
        if update {
            std::fs::create_dir_all(golden_dir()).unwrap();
            std::fs::write(&path, &actual).unwrap();
            continue;
        }
        let want = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| {
                panic!(
                    "{}: {e}; create it with FTAR_UPDATE_GOLDEN=1",
                    path.display()
                )
            })
            // git may check the file out with CRLF line ends
            .replace("\r\n", "\n");
        if want != actual.replace("\r\n", "\n") {
            failed.push(format!("{name} (ftar {})", args.join(" ")));
        }
    }
    assert!(
        failed.is_empty(),
        "output differs from tests/golden for: {failed:#?}\nif intended, regenerate with \
         FTAR_UPDATE_GOLDEN=1 and review the diff"
    );
}
