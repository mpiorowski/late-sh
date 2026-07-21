use crate::app::common::marquee::*;

#[test]
fn marquee_returns_text_that_fits_unchanged() {
    assert_eq!(marquee_text("short", 10, 42), "short");
}

#[test]
fn marquee_holds_at_start_then_scrolls() {
    // 8 chars in a 5-col window: travel 3, hold 20, step 3.
    assert_eq!(marquee_text("abcdefgh", 5, 0), "abcde");
    assert_eq!(marquee_text("abcdefgh", 5, 19), "abcde");
    assert_eq!(marquee_text("abcdefgh", 5, 23), "bcdef");
}
