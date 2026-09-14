use super::{PULSE_BUCKET_SECS, PULSE_BUCKETS, PulseHistory};
use chrono::{DateTime, Duration, Utc};

fn at(bucket: i64, offset_secs: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(bucket * PULSE_BUCKET_SECS + offset_secs, 0).expect("valid time")
}

#[test]
fn a_day_of_pulse_keeps_each_buckets_peak_and_forgets_what_is_older() {
    let start = 1_000_000;
    let mut pulse = PulseHistory::default();
    // Two samples in the first bucket: the peak stays.
    pulse.record(at(start, 10), 4);
    pulse.record(at(start, 300), 9);
    pulse.record(at(start, 590), 2);
    // Nothing sampled in the next bucket, then a quiet one.
    pulse.record(at(start + 2, 0), 3);

    let now = at(start + 2, 30);
    let series = pulse.series(now);
    assert_eq!(series.len(), PULSE_BUCKETS);
    let mut expected = vec![None; PULSE_BUCKETS];
    expected[PULSE_BUCKETS - 3] = Some(9);
    expected[PULSE_BUCKETS - 1] = Some(3);
    assert_eq!(series, expected);

    // A day later the first bucket has scrolled off; the last sample sits
    // at the oldest end until its own day is up.
    let later = at(start + PULSE_BUCKETS as i64 + 1, 0);
    pulse.record(later, 5);
    let series = pulse.series(later);
    let mut expected = vec![None; PULSE_BUCKETS];
    expected[0] = Some(3);
    expected[PULSE_BUCKETS - 1] = Some(5);
    assert_eq!(series, expected);
    assert_eq!(
        pulse.series(later + Duration::days(2)),
        vec![None; PULSE_BUCKETS],
        "a history nobody records into reads empty once its day passes"
    );
}
