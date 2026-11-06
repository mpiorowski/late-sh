use chrono::{Duration, TimeZone, Utc};

use crate::metrics::Tenure;

/// An account is `DayOne` for its first 24 hours, `FirstWeek` until the
/// seventh day, `Regular` from then on; the edges belong to the older
/// bucket.
#[test]
fn tenure_buckets_an_account_by_its_age() {
    let created = Utc.with_ymd_and_hms(2026, 10, 2, 9, 30, 0).unwrap();
    let at = |age: Duration| Tenure::of(created, created + age);

    assert_eq!(at(Duration::zero()), Tenure::DayOne);
    assert_eq!(at(Duration::hours(23)), Tenure::DayOne);
    assert_eq!(at(Duration::days(1)), Tenure::FirstWeek);
    assert_eq!(
        at(Duration::days(6) + Duration::hours(23)),
        Tenure::FirstWeek
    );
    assert_eq!(at(Duration::days(7)), Tenure::Regular);
    assert_eq!(at(Duration::days(400)), Tenure::Regular);
}
