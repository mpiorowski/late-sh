use chrono::{Duration, NaiveDate, Utc};
use tokio_postgres::Client;

use super::herald::{
    HeraldDelivery, HeraldKind, HeraldNetwork, HeraldPost, HeraldPosted, HeraldSwitch,
};
use crate::test_utils::test_db;

const DAY: NaiveDate = match NaiveDate::from_ymd_opt(2026, 10, 23) {
    Some(date) => date,
    None => unreachable!(),
};

/// The sweeper's attempt cap, as the tests here drive it.
const MAX_ATTEMPTS: i32 = 3;

/// A stale bound in the past: nothing claimed during the test is stale.
fn fresh() -> chrono::DateTime<Utc> {
    Utc::now() - Duration::minutes(20)
}

/// Store the pick and settle the day as posted on Mastodon.
async fn post(client: &Client, day: NaiveDate, kind: HeraldKind, subject: &str) {
    HeraldPost::store_picture(
        client,
        day,
        kind,
        subject,
        &serde_json::json!({"pick": subject}),
    )
    .await
    .expect("store pick");
    HeraldPost::record_delivery(
        client,
        day,
        HeraldNetwork::Mastodon,
        "https://instance.example/@late/1",
    )
    .await
    .expect("record delivery");
    HeraldPost::finish_posted(client, day)
        .await
        .expect("finish");
}

/// One replica takes the day and keeps its key; a second finds nothing;
/// a settled day reads finished and is never claimed again.
#[tokio::test]
async fn a_day_is_claimed_once_and_settles() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");

    let key = HeraldPost::claim(&client, DAY, fresh(), MAX_ATTEMPTS)
        .await
        .expect("claim")
        .expect("first claim wins");
    assert_eq!(
        HeraldPost::claim(&client, DAY, fresh(), MAX_ATTEMPTS)
            .await
            .expect("claim"),
        None,
        "a posting day is held"
    );
    assert!(
        !HeraldPost::is_finished(&client, DAY, MAX_ATTEMPTS)
            .await
            .expect("read"),
        "held is not finished"
    );

    post(&client, DAY, HeraldKind::Artboard, "daily:2026-10-22").await;
    assert!(
        HeraldPost::is_finished(&client, DAY, MAX_ATTEMPTS)
            .await
            .expect("read")
    );
    assert_eq!(
        HeraldPost::claim(&client, DAY, Utc::now() + Duration::hours(1), MAX_ATTEMPTS)
            .await
            .expect("claim"),
        None,
        "a posted day is never reclaimed, however old"
    );
    let record = HeraldPost::posted_on(&client, DAY)
        .await
        .expect("read")
        .expect("posted");
    assert_eq!(
        record,
        HeraldPosted {
            kind: HeraldKind::Artboard,
            subject: "daily:2026-10-22".to_string(),
            deliveries: vec![HeraldDelivery {
                network: HeraldNetwork::Mastodon,
                url: "https://instance.example/@late/1".to_string(),
            }],
        }
    );
    assert_eq!(
        HeraldPost::stored_picture(&client, DAY)
            .await
            .expect("read"),
        Some(serde_json::json!({"pick": "daily:2026-10-22"}))
    );

    // The key is the first claim's and never changes; a pick and a
    // delivery that landed before the day failed survive the retry, and
    // a second record of the same network is ignored.
    let later = DAY + Duration::days(1);
    let first = HeraldPost::claim(&client, later, fresh(), MAX_ATTEMPTS)
        .await
        .expect("claim")
        .expect("wins");
    HeraldPost::store_picture(
        &client,
        later,
        HeraldKind::Artboard,
        "daily:2026-10-23",
        &serde_json::json!({}),
    )
    .await
    .expect("store pick");
    HeraldPost::record_delivery(&client, later, HeraldNetwork::Bluesky, "https://bsky.app/1")
        .await
        .expect("record delivery");
    HeraldPost::record_delivery(&client, later, HeraldNetwork::Bluesky, "https://bsky.app/2")
        .await
        .expect("record delivery again");
    HeraldPost::mark_failed(&client, later).await.expect("fail");
    let retry = HeraldPost::claim(&client, later, fresh(), MAX_ATTEMPTS)
        .await
        .expect("claim")
        .expect("a failed day under the cap is claimed again");
    assert_eq!(retry, first, "the idempotency key survives a retry");
    assert_ne!(key, first, "each day has its own key");
    assert_eq!(
        HeraldPost::stored_picture(&client, later)
            .await
            .expect("read"),
        Some(serde_json::json!({}))
    );
    assert_eq!(
        HeraldPost::deliveries(&client, later).await.expect("read"),
        vec![HeraldDelivery {
            network: HeraldNetwork::Bluesky,
            url: "https://bsky.app/1".to_string(),
        }]
    );
    assert_eq!(
        HeraldPost::posted_on(&client, later).await.expect("read"),
        None,
        "a delivery is not a posted day"
    );
}

/// A failed day is retried up to the cap and then rests; a claim nobody
/// settled is taken over once stale; a skipped day is finished.
#[tokio::test]
async fn failures_retry_to_the_cap_and_stale_claims_are_taken_over() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");

    for attempt in 1..=MAX_ATTEMPTS {
        HeraldPost::claim(&client, DAY, fresh(), MAX_ATTEMPTS)
            .await
            .expect("claim")
            .unwrap_or_else(|| panic!("attempt {attempt} claims"));
        HeraldPost::mark_failed(&client, DAY).await.expect("fail");
    }
    assert_eq!(
        HeraldPost::claim(&client, DAY, fresh(), MAX_ATTEMPTS)
            .await
            .expect("claim"),
        None,
        "at the cap the day rests"
    );
    assert!(
        HeraldPost::is_finished(&client, DAY, MAX_ATTEMPTS)
            .await
            .expect("read")
    );

    let stuck = DAY + Duration::days(1);
    HeraldPost::claim(&client, stuck, fresh(), MAX_ATTEMPTS)
        .await
        .expect("claim")
        .expect("wins");
    assert_eq!(
        HeraldPost::claim(&client, stuck, fresh(), MAX_ATTEMPTS)
            .await
            .expect("claim"),
        None,
        "a live claim is held"
    );
    let stale_bound_in_the_future = Utc::now() + Duration::minutes(1);
    HeraldPost::claim(&client, stuck, stale_bound_in_the_future, MAX_ATTEMPTS)
        .await
        .expect("claim")
        .expect("a stale claim is taken over");

    HeraldPost::finish_skipped(&client, stuck)
        .await
        .expect("skip");
    assert!(
        HeraldPost::is_finished(&client, stuck, MAX_ATTEMPTS)
            .await
            .expect("read"),
        "a skipped day is finished"
    );
    assert_eq!(
        HeraldPost::posted_on(&client, stuck).await.expect("read"),
        None
    );
}

/// The switch is seeded off and reads back whatever was last written to
/// its row.
#[tokio::test]
async fn the_switch_is_seeded_off_and_flips() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");

    assert!(
        !HeraldSwitch::is_on(&client)
            .await
            .expect("read seeded switch")
    );

    HeraldSwitch::set(&client, true).await.expect("flip on");
    assert!(
        HeraldSwitch::is_on(&client)
            .await
            .expect("read flipped switch")
    );
}
