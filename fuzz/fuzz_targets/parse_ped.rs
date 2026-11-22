//! `.ped` primary event database parser: never panics; parsed databases can be written back.
#![no_main]

use ftar_core::{parse::parse_ped, write::write_ped};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(db) = parse_ped(data) {
        let text = write_ped(&db);
        let _ = parse_ped(text.as_bytes());
    }
});
