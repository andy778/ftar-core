//! Cut sets and probability on untrusted trees and databases.
//! Input: `<.fta bytes> NUL <.ped bytes>`.
#![no_main]

use ftar_core::{
    analysis::{minimal_cut_sets_limited, top_probability},
    parse::{parse_fta, parse_ped},
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
    if let Ok(cut_sets) = minimal_cut_sets_limited(&tree, MAX_ORDER) {
        let _ = top_probability(&cut_sets, &db);
    }
});
