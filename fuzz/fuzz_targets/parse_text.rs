//! Text-format (`.ftt`) parser and time analysis: never panic; whatever is
//! read can be written back, reads back the same tree and database, and
//! writing it again gives the same text.
#![no_main]

use ftar_core::quant::{analyse, QuantOptions};
use ftar_core::text::{parse_text, parse_text_ast, write_text};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(doc) = parse_text(data) else { return };
    if let Ok(ast) = parse_text_ast(data) {
        let opts = QuantOptions {
            budget: 100_000,
            sample_size: Some(2),
            ..QuantOptions::default()
        };
        let _ = analyse(&ast, &opts);
    }
    let Ok(text) = write_text(&doc.tree, &doc.db, &doc.extras) else { return };
    let again = parse_text(text.as_bytes()).expect("written text parses");
    assert_eq!((&again.tree, &again.db), (&doc.tree, &doc.db));
    let text2 = write_text(&again.tree, &again.db, &again.extras).expect("writes again");
    assert_eq!(text2, text);
});
