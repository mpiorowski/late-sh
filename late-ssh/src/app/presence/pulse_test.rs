use chrono::{Duration, TimeZone, Utc};
use late_core::models::presence_sample::HeadcountPoint;

use super::{GAP, columns, glyphs, height, peak};

fn point(hours_ago: i64, online: i64) -> HeadcountPoint {
    HeadcountPoint {
        bucket: now() - Duration::hours(hours_ago),
        online,
    }
}

fn now() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 2, 22, 0, 0).unwrap()
}

#[test]
fn the_day_is_cut_into_columns_that_keep_the_highest_sample_and_mark_gaps() {
    let points = vec![
        point(23, 2),
        point(12, 7),
        point(12, 4),
        point(1, 5),
        point(30, 99),
    ];
    let cols = columns(&points, now(), 4);
    // Six hours a column: 23h ago is the first, 12h ago lands on the
    // boundary of the third, 1h ago the last; 30h ago is outside the day.
    assert_eq!(cols, vec![Some(2), None, Some(7), Some(5)]);
    assert_eq!(columns(&points, now(), 0), Vec::<Option<i64>>::new());
}

#[test]
fn the_peak_is_the_highest_sample_the_latest_on_a_tie() {
    let points = vec![point(5, 7), point(3, 7), point(1, 2)];
    assert_eq!(peak(&points), Some(point(3, 7)));
    assert_eq!(peak(&[]), None);
}

#[test]
fn a_column_climbs_in_eighths_from_the_bottom_row() {
    // Two rows: sixteen eighths. Half of the peak is a full bottom row.
    assert_eq!(glyphs(Some(8), 16, 2), vec!['█', ' ']);
    assert_eq!(glyphs(Some(16), 16, 2), vec!['█', '█']);
    assert_eq!(glyphs(Some(11), 16, 2), vec!['█', '▃']);
    // One person is one step, never nothing; a zero is blank; a gap is
    // the dot on the bottom row.
    assert_eq!(glyphs(Some(1), 400, 2), vec!['▁', ' ']);
    assert_eq!(glyphs(Some(0), 16, 2), vec![' ', ' ']);
    assert_eq!(glyphs(None, 16, 2), vec![GAP, ' ']);
    assert_eq!(glyphs(Some(5), 16, 0), Vec::<char>::new());
}

#[test]
fn a_column_s_height_is_its_share_of_the_peak() {
    assert_eq!(height(Some(4), 8), 0.5);
    assert_eq!(height(Some(12), 8), 1.0);
    assert_eq!(height(None, 8), 0.0);
    assert_eq!(height(Some(3), 0), 0.0);
}
