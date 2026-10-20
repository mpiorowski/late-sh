use super::visible_window_start;

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
