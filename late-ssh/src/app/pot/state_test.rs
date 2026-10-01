use chrono::{TimeZone, Utc};

use super::{countdown, lead_time, short_duration};

/// The one countdown format the pot's copy uses, in every shape it has. A due
/// pot reads `soon` rather than `0s` or a negative: the sweeper wakes once a
/// minute, so "now" would be a promise the panel cannot keep.
#[test]
fn the_countdown_reads_the_five_shapes_it_has() {
    assert_eq!(short_duration(4 * 86_400 + 12 * 3_600 + 30 * 60), "4d12h");
    assert_eq!(short_duration(86_400), "1d00h");
    assert_eq!(short_duration(3 * 3_600 + 12 * 60), "3h12m");
    assert_eq!(short_duration(12 * 60), "12m");
    assert_eq!(short_duration(45), "45s");
    assert_eq!(short_duration(0), "soon");
    assert_eq!(short_duration(-90), "soon");
    // The minutes are zero-padded so the row never changes width inside an
    // hour: `3h02m`, not `3h2m`.
    assert_eq!(short_duration(3 * 3_600 + 2 * 60), "3h02m");
}

#[test]
fn the_countdown_measures_to_the_draw() {
    let now = Utc.with_ymd_and_hms(2026, 8, 27, 17, 48, 0).unwrap();
    let draws_at = Utc.with_ymd_and_hms(2026, 8, 27, 21, 0, 0).unwrap();
    assert_eq!(countdown(draws_at, now), "3h12m");
}

/// The last call is a sentence: a whole hour reads `1h`, and a call that went
/// out late in the window (a buy after it opened) reads like the badge.
#[test]
fn the_last_call_says_a_whole_hour_plainly() {
    assert_eq!(lead_time(3_600), "1h");
    assert_eq!(lead_time(2 * 3_600), "2h");
    assert_eq!(lead_time(42 * 60), "42m");
    assert_eq!(lead_time(3_600 + 5 * 60), "1h05m");
    assert_eq!(lead_time(86_400), "1d00h");
}
