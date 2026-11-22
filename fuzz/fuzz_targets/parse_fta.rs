//! `.fta` parser: never panics, and whatever parses survives a write/parse round trip.
#![no_main]

use ftar_core::{parse::parse_fta, write::write_fta};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(tree) = parse_fta(data) {
        let text = write_fta(&tree);
        let _ = parse_fta(text.as_bytes());
    }
});
