//! Text-format (`.ftt`) parser: never panics; whatever it reads can be written
//! back and reads back the same.
#![no_main]

use ftar_core::text::{parse_text, write_text};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok((tree, db)) = parse_text(data) {
        if let Ok(text) = write_text(&tree, &db) {
            let again = parse_text(text.as_bytes()).expect("written text parses");
            assert_eq!(again, (tree, db));
        }
    }
});
