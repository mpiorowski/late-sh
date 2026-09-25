use crate::app::common::marquee::*;

#[test]
fn marquee_returns_text_that_fits_unchanged() {
    assert_eq!(marquee_text("short", 10, 42), "short");
}

#[test]
fn marquee_holds_at_start_then_scrolls() {
    // 12 chars in a 5-col window: travel 7, hold 45, step 15, 3 cols/step.
    assert_eq!(marquee_text("abcdefghijkl", 5, 0), "abcde");
    assert_eq!(marquee_text("abcdefghijkl", 5, 44), "abcde");
    assert_eq!(marquee_text("abcdefghijkl", 5, 60), "defgh");
    assert_eq!(marquee_text("abcdefghijkl", 5, 75), "ghijk");
    // Offset clamps at travel, so the last step shows the tail exactly.
    assert_eq!(marquee_text("abcdefghijkl", 5, 90), "hijkl");
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

/// The away glyph on the friends row paints two columns: the window is
/// measured in columns, so the row never overruns its rail, and a wide glyph
/// cut by an edge becomes a space instead of spilling past it.
#[test]
fn marquee_measures_wide_glyphs_in_columns() {
    assert!(!marquee_scrolls("ab 💤", 5), "exactly five columns fits");
    assert!(marquee_scrolls("abc 💤", 5));
    assert_eq!(
        marquee_text("abc 💤", 5, 0),
        "abc  ",
        "cut at the right edge"
    );
    assert_eq!(
        marquee_text("ab💤cdefg", 5, 60),
        " cdef",
        "cut at the left edge"
    );
    for tick in 0..400 {
        let window = marquee_text("@ada 💤 @bob 💤 @cyd", 7, tick);
        assert_eq!(
            unicode_width::UnicodeWidthStr::width(window.as_str()),
            7,
            "tick {tick} painted {window:?}"
        );
    }
}
