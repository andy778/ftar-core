//! Report header timestamps keep C `ctime` layout after replacing chrono (#36).

use ftar_gui::{ctime_text, App};

fn at(s: &str) -> String {
    ctime_text(&s.parse().expect("valid zoned time"))
}

#[test]
fn single_digit_day_is_space_padded() {
    assert_eq!(at("2024-01-01T12:00:00[UTC]"), "Mon Jan  1 12:00:00 2024");
}

#[test]
fn two_digit_day_and_zero_padded_time() {
    assert_eq!(at("2026-11-22T09:05:07[UTC]"), "Sun Nov 22 09:05:07 2026");
}

#[test]
fn now_text_has_ctime_shape() {
    let s = App::now_text();
    assert_eq!(s.len(), 24, "{s:?}");
    assert_eq!(&s[3..4], " ");
    assert_eq!(&s[7..8], " ");
    assert_eq!(&s[13..14], ":");
    assert_eq!(&s[16..17], ":");
}
