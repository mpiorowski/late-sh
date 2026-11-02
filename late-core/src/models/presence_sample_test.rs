use chrono::{Duration, TimeZone, Utc};
use uuid::Uuid;

use crate::{
    models::presence_sample::{HeadcountPoint, PresenceSample},
    test_utils::test_db,
};

#[test]
fn a_bucket_is_the_clock_floored_to_five_minutes() {
    let at = Utc.with_ymd_and_hms(2026, 11, 2, 12, 7, 33).unwrap();
    assert_eq!(
        PresenceSample::bucket_of(at),
        Utc.with_ymd_and_hms(2026, 11, 2, 12, 5, 0).unwrap()
    );
}

/// Two replicas in one bucket add up, a replica's second write for the
/// bucket replaces its first, and a bucket older than a day is not read.
#[tokio::test]
async fn replicas_sum_per_bucket_and_a_day_is_the_window() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let now = Utc::now();
    let bucket = PresenceSample::bucket_of(now);
    let replica_a = Uuid::now_v7();
    let replica_b = Uuid::now_v7();
    PresenceSample::record(&client, replica_a, bucket, 3)
        .await
        .expect("record");
    PresenceSample::record(&client, replica_b, bucket, 4)
        .await
        .expect("record");
    PresenceSample::record(&client, replica_a, bucket, 5)
        .await
        .expect("record again");
    let stale = bucket - Duration::hours(25);
    PresenceSample::record(&client, replica_a, stale, 9)
        .await
        .expect("record stale");

    let points = PresenceSample::last_day(&client, now).await.expect("read");
    assert_eq!(points, vec![HeadcountPoint { bucket, online: 9 }]);
}
