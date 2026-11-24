//! ftar's text reports against reports made by the original OpenFTA engine
//! (issue #12). The fixtures in `tests/data/openfta/` come from a native
//! build of OpenFTA's C core (`tools/openfta-native/`): a cut sets report
//! and a probability report for every example tree in `tests/data`.
//!
//! Layout must match exactly. Numbers may differ in the last digits, because
//! OpenFTA computes in `float` and ftar in `f64` (docs/VERIFICATION.md, item 7).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};

use ftar_core::project::Project;
use ftar_core::report::{generate, ReportKind, ReportParams};

const DATA: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data");

/// Relative tolerance for numbers. OpenFTA's `float` keeps about 7 digits
/// (eps 6e-8), and an inclusion-exclusion term adds one product per
/// combination of cut sets in `float`: 165 of them for 3 terms of T.fta's 11
/// cut sets, so ~1e-5 of rounding. Seen: 7.3e-6 there, 3e-6 for Three Motor.
const REL: f64 = 1e-5;

/// The value of a single inclusion-exclusion term (`+8.273107E-06` on an
/// `N terms` line) is a `float` sum of C(cut sets, N) products: 316251 for 4
/// terms of Three Motor's 54 cut sets, where it differs by 2.4e-5. The
/// running total on the same line must still agree within [`REL`].
const REL_TERM: f64 = 1e-4;

/// Every example tree, with the fixture name `generate.sh` gives it.
fn trees() -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    let mut dirs = vec![PathBuf::from(DATA)];
    while let Some(d) = dirs.pop() {
        for e in std::fs::read_dir(&d).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                if p.file_name().is_some_and(|n| n != "openfta") {
                    dirs.push(p);
                }
            } else if p.extension().is_some_and(|x| x == "fta") {
                let rel = p.strip_prefix(DATA).unwrap().with_extension("");
                let rel = rel.to_string_lossy().replace('\\', "/");
                let name: String = rel
                    .chars()
                    .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
                    .collect();
                out.push((name, p));
            }
        }
    }
    out.sort();
    out
}

fn fixture(name: &str, kind: &str) -> String {
    let p = Path::new(DATA)
        .join("openfta")
        .join(format!("{name}.{kind}.txt"));
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

/// The number after `prefix` on the line that starts with it.
fn value(text: &str, prefix: &str) -> usize {
    text.lines()
        .find_map(|l| l.strip_prefix(prefix))
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or_else(|| panic!("no `{prefix}` in\n{text}"))
}

fn number(tok: &str) -> Option<f64> {
    let t = tok.trim_start_matches('+').trim_end_matches('%');
    if t.chars().any(|c| c.is_ascii_digit()) {
        t.parse().ok()
    } else {
        None
    }
}

/// Lines equal, except that numbers may differ by [`REL`] ([`REL_TERM`] for
/// a term's own value; percentages, printed with two decimals, by one in the
/// last place).
fn same(ours: &str, theirs: &str) -> Result<(), String> {
    let (a, b): (Vec<_>, Vec<_>) = (ours.lines().collect(), theirs.lines().collect());
    for (i, (x, y)) in a.iter().zip(&b).enumerate() {
        if x == y {
            continue;
        }
        let term_line = x
            .trim_start()
            .split(' ')
            .nth(1)
            .is_some_and(|w| w.starts_with("term"));
        let shape = |l: &str| -> String {
            l.split(' ')
                .map(|t| if number(t).is_some() { "#" } else { t })
                .collect::<Vec<_>>()
                .join(" ")
        };
        let close = shape(x) == shape(y)
            && x.split(' ')
                .zip(y.split(' '))
                .all(|(p, q)| match (number(p), number(q)) {
                    (Some(u), Some(v)) => {
                        let rel = if term_line && (p.starts_with('+') || p.starts_with('-')) {
                            REL_TERM
                        } else {
                            REL
                        };
                        let tol = if p.ends_with('%') {
                            0.011
                        } else {
                            rel * u.abs().max(v.abs())
                        };
                        (u - v).abs() <= tol
                    }
                    _ => p == q,
                });
        if !close {
            return Err(format!(
                "line {}:\n  ftar:    {x:?}\n  OpenFTA: {y:?}",
                i + 1
            ));
        }
    }
    if a.len() != b.len() {
        return Err(format!("ftar has {} lines, OpenFTA {}", a.len(), b.len()));
    }
    Ok(())
}

fn ours(path: &Path, kind: ReportKind, params: &ReportParams) -> String {
    let p = Project::load(path).unwrap();
    let name = path.file_name().unwrap().to_string_lossy();
    generate(kind, &p.tree, &p.db, &name, "", params).unwrap()
}

#[test]
fn every_example_has_fixtures() {
    let t = trees();
    assert!(t.len() >= 10, "{t:?}");
    for (name, _) in &t {
        fixture(name, "cutsets");
        fixture(name, "probability");
    }
}

#[test]
fn cut_sets_reports_match_openfta() {
    let mut failures = Vec::new();
    for (name, path) in trees() {
        let want = fixture(&name, "cutsets");
        let params = ReportParams {
            max_order: value(&want, "Minimal cut set order = 1 to "),
            ..Default::default()
        };
        if let Err(e) = same(&ours(&path, ReportKind::CutSets, &params), &want) {
            failures.push(format!("{name}: {e}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

#[test]
fn probability_reports_match_openfta() {
    let mut failures = Vec::new();
    for (name, path) in trees() {
        let want = fixture(&name, "probability");
        let terms = want
            .lines()
            .filter(|l| l.contains(" term") && l.ends_with("bound)"))
            .count();
        let mut params = ReportParams {
            max_order: value(&want, "Order of minimal cut sets  = "),
            terms,
            ..Default::default()
        };
        params.monte_carlo.unit_time = 1.0;
        if let Err(e) = same(&ours(&path, ReportKind::Probability, &params), &want) {
            failures.push(format!("{name}: {e}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

/// The comparison itself catches differences.
#[test]
fn comparison_notices_layout_and_number_changes() {
    assert!(same("a 1.000000E-01", "a 1.000000E-01").is_ok());
    assert!(same("a 1.000001E-01", "a 1.000000E-01").is_ok());
    assert!(same("a 1.000200E-01", "a 1.000000E-01").is_err());
    assert!(same("a  1.0", "a 1.0").is_err());
    assert!(same("x\ny", "x").is_err());
    assert!(same(" 28.78%", " 28.79%").is_ok());
    assert!(same(" 28.78%", " 28.90%").is_err());
}

/// Only a term's own value gets the looser tolerance, not the running total.
#[test]
fn term_values_alone_get_the_term_tolerance() {
    let ours = " 4 terms   -2.704429E-07   = 2.720230E-03 (lower bound)";
    assert!(same(
        ours,
        " 4 terms   -2.704493E-07   = 2.720231E-03 (lower bound)"
    )
    .is_ok());
    assert!(same(
        ours,
        " 4 terms   -2.704429E-07   = 2.720330E-03 (lower bound)"
    )
    .is_err());
    assert!(same(
        ours,
        " 4 terms   -2.714429E-07   = 2.720230E-03 (lower bound)"
    )
    .is_err());
}
