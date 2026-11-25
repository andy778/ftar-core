//! Cut sets, probability, Monte Carlo and every text report on untrusted trees
//! and databases. Input: `<.fta bytes> NUL <.ped bytes>`.
//!
//! Besides "no panic", every number must be finite: a NaN or inf in a report is
//! a division by zero upstream (OpenFTA's Monte Carlo has those on small trees,
//! docs/VERIFICATION.md item 21).
#![no_main]

use ftar_core::{
    analysis::{minimal_cut_sets_limited, top_probability},
    montecarlo::{Method, MonteCarloOptions},
    parse::{parse_fta, parse_ped},
    report::{generate, ReportKind, ReportParams},
};
use libfuzzer_sys::fuzz_target;

/// Keeps the (exponential) cut set expansion within the fuzzer's time budget.
const MAX_ORDER: usize = 4;

fuzz_target!(|data: &[u8]| {
    let (fta, ped) = match data.iter().position(|&b| b == 0) {
        Some(i) => (&data[..i], &data[i + 1..]),
        None => (data, &[][..]),
    };
    let Ok(tree) = parse_fta(fta) else { return };
    let db = parse_ped(ped).unwrap_or_default();
    let Ok(cut_sets) = minimal_cut_sets_limited(&tree, MAX_ORDER) else {
        return;
    };
    if let Ok(top) = top_probability(&cut_sets, &db) {
        let s = format!("{top:?}");
        assert!(!s.contains("NaN") && !s.contains("inf"), "{s}");
    }

    // The last input byte picks the Monte Carlo set-up: 0 to 63 trials, either method.
    let knob = data.last().copied().unwrap_or(0);
    let params = ReportParams {
        max_order: MAX_ORDER,
        terms: 3,
        monte_carlo: MonteCarloOptions {
            trials: u64::from(knob & 0x3f),
            method: if knob & 0x40 == 0 {
                Method::FirstFailure
            } else {
                Method::Direct
            },
            ..MonteCarloOptions::default()
        },
    };
    // `fmt_e` prints NAN / INF, `{:.2}` NaN / inf. Ids come from the input, so
    // skip the text check when the input itself spells one of them.
    let spelled = String::from_utf8_lossy(fta).to_lowercase();
    let check_text = !spelled.contains("nan") && !spelled.contains("inf");
    for kind in ReportKind::ALL {
        if let Ok(text) = generate(kind, &tree, &db, "fuzz", "", &params) {
            let lower = text.to_lowercase();
            assert!(
                !check_text || (!lower.contains("nan") && !lower.contains("inf")),
                "{kind:?}:\n{text}"
            );
        }
    }
});
