use super::*;

#[test]
fn visible_window_start_keeps_selected_item_visible() {
    assert_eq!(visible_window_start(0, 20, 5), 0);
    assert_eq!(visible_window_start(3, 20, 5), 1);
    assert_eq!(visible_window_start(19, 20, 5), 15);
}

#[test]
fn pad_display_width_handles_variation_selector_emoji() {
    let padded = pad_display_width("☀️", 6);
    assert_eq!(UnicodeWidthStr::width(padded.as_str()), 6);
    let padded = pad_display_width("🐱", 6);
    assert_eq!(UnicodeWidthStr::width(padded.as_str()), 6);
}

#[test]
fn remaining_label_floors_at_one_minute() {
    use chrono::{Duration, Utc};
    let now = Utc::now();
    assert_eq!(remaining_label(now + Duration::hours(17), now), "17h left");
    assert_eq!(
        remaining_label(now + Duration::minutes(59), now),
        "59m left"
    );
    assert_eq!(remaining_label(now + Duration::seconds(5), now), "1m left");
    assert_eq!(remaining_label(now - Duration::minutes(3), now), "1m left");
}
