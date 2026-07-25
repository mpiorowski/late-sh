use super::{col, visible_window_start};

#[test]
fn list_window_stays_at_the_top_until_the_cursor_reaches_the_middle() {
    // 40 lines in a 10-row window: the first five selections scroll nothing,
    // so the cursor walks down the list instead of the list walking past it.
    for selected in 0..=5 {
        assert_eq!(visible_window_start(selected, 40, 10), 0);
    }
    assert_eq!(visible_window_start(6, 40, 10), 1);
}

#[test]
fn list_window_keeps_rows_below_the_selection() {
    let start = visible_window_start(20, 40, 10);
    // The selection sits mid-window, so four more rows are drawn under it.
    assert_eq!(start, 15);
    assert!(20 < start + 10 - 1);
}

#[test]
fn list_window_stops_at_the_last_page() {
    assert_eq!(visible_window_start(39, 40, 10), 30);
}

#[test]
fn list_window_does_not_scroll_a_list_that_fits() {
    assert_eq!(visible_window_start(7, 10, 10), 0);
}

#[test]
fn col_pads_short_text_to_width() {
    assert_eq!(col("chess", 8), "chess   ");
}

#[test]
fn col_always_leaves_a_gap_before_the_next_column() {
    // Exactly width chars would swallow the separator; the longest
    // fitting content is width - 1 chars plus one space.
    assert_eq!(col("12345678", 8), "123456… ");
    assert_eq!(col("1234567", 8), "1234567 ");
    assert_eq!(col("challenges @kirii.md", 18), "challenges @kiri… ");
}

#[test]
fn col_counts_chars_not_bytes() {
    assert_eq!(col("héllo", 8), "héllo   ");
}
