use crate::app::common::marquee::*;

#[test]
fn marquee_returns_text_that_fits_unchanged() {
    assert_eq!(marquee_text("short", 10, 42), "short");
}

#[test]
fn marquee_holds_at_start_then_scrolls() {
    // 8 chars in a 5-col window: travel 3, hold 45, step 15.
    assert_eq!(marquee_text("abcdefgh", 5, 0), "abcde");
    assert_eq!(marquee_text("abcdefgh", 5, 44), "abcde");
    assert_eq!(marquee_text("abcdefgh", 5, 60), "bcdef");
}

#[test]
fn marquee_transitions_land_on_step_boundaries() {
    // The render gate only paints marquees on multiples of
    // MARQUEE_STEP_TICKS, so the window must never move between them.
    let mut previous = marquee_text("abcdefgh", 5, 0);
    for tick in 1..400 {
        let current = marquee_text("abcdefgh", 5, tick);
        if !tick.is_multiple_of(MARQUEE_STEP_TICKS) {
            assert_eq!(
                current, previous,
                "window moved off-boundary at tick {tick}"
            );
        }
        previous = current;
    }
}

#[test]
fn marquee_scrolls_only_for_overflowing_text() {
    assert!(!marquee_scrolls("short", 10));
    assert!(!marquee_scrolls("exact", 5));
    assert!(marquee_scrolls("abcdefgh", 5));
    assert!(!marquee_scrolls("abcdefgh", 0));
}
