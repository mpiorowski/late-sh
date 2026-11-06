use chrono::{Duration, NaiveDate, Utc};
use tokio_postgres::Client;

use super::herald::{
    HeraldDayStatus, HeraldDelivery, HeraldKind, HeraldNetwork, HeraldPick, HeraldPicked,
    HeraldPost, HeraldPosted, HeraldSlot, HeraldSwitch,
};
use crate::test_utils::test_db;

const DAY: NaiveDate = match NaiveDate::from_ymd_opt(2026, 10, 23) {
    Some(date) => date,
    None => unreachable!(),
};

/// The slot most tests here drive; `each_slot_has_its_own_claim` drives both.
const SLOT: HeraldSlot = HeraldSlot::Art;

/// The checker's attempt cap, as the tests here drive it.
const MAX_ATTEMPTS: i32 = 3;

/// A stale bound in the past: nothing claimed during the test is stale.
fn fresh() -> chrono::DateTime<Utc> {
    Utc::now() - Duration::minutes(20)
}

/// The window's start: every first claim made during the test is inside.
fn window() -> chrono::DateTime<Utc> {
    Utc::now() - Duration::minutes(50)
}

/// Store the pick and settle the day as posted on Mastodon.
async fn post(client: &Client, day: NaiveDate, kind: HeraldKind, subject: &str) {
    HeraldPost::store_pick(
        client,
        day,
        SLOT,
        kind,
        subject,
        &HeraldPick {
            picture: serde_json::json!({"pick": subject}),
            look: serde_json::json!("unseen"),
        },
    )
    .await
    .expect("store pick");
    HeraldPost::record_delivery(
        client,
        day,
        SLOT,
        HeraldNetwork::Mastodon,
        "https://instance.example/@late/1",
    )
    .await
    .expect("record delivery");
    HeraldPost::finish_posted(client, day, SLOT)
        .await
        .expect("finish");
}

/// One replica takes the day and keeps its key; a second finds nothing;
/// a settled day reads finished and is never claimed again.
#[tokio::test]
async fn a_day_is_claimed_once_and_settles() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");

    let key = HeraldPost::claim(&client, DAY, SLOT, fresh(), window(), MAX_ATTEMPTS)
        .await
        .expect("claim")
        .expect("first claim wins");
    assert_eq!(
        HeraldPost::claim(&client, DAY, SLOT, fresh(), window(), MAX_ATTEMPTS)
            .await
            .expect("claim"),
        None,
        "a posting day is held"
    );
    assert!(
        !HeraldPost::is_finished(&client, DAY, SLOT, window(), MAX_ATTEMPTS)
            .await
            .expect("read"),
        "held is not finished"
    );

    post(&client, DAY, HeraldKind::Artboard, "daily:2026-10-22").await;
    assert!(
        HeraldPost::is_finished(&client, DAY, SLOT, window(), MAX_ATTEMPTS)
            .await
            .expect("read")
    );
    assert_eq!(
        HeraldPost::claim(
            &client,
            DAY,
            SLOT,
            Utc::now() + Duration::hours(1),
            window(),
            MAX_ATTEMPTS
        )
        .await
        .expect("claim"),
        None,
        "a posted day is never reclaimed, however old"
    );
    let record = HeraldPost::posted_on(&client, DAY, SLOT)
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
        HeraldPost::stored_pick(&client, DAY, SLOT)
            .await
            .expect("read"),
        Some(HeraldPick {
            picture: serde_json::json!({"pick": "daily:2026-10-22"}),
            look: serde_json::json!("unseen"),
        })
    );

    // The key is the first claim's and never changes; a pick and a
    // delivery that landed before the day failed survive the retry, and
    // a second record of the same network is ignored.
    let later = DAY + Duration::days(1);
    let first = HeraldPost::claim(&client, later, SLOT, fresh(), window(), MAX_ATTEMPTS)
        .await
        .expect("claim")
        .expect("wins");
    HeraldPost::store_pick(
        &client,
        later,
        SLOT,
        HeraldKind::Artboard,
        "daily:2026-10-23",
        &HeraldPick {
            picture: serde_json::json!({}),
            look: serde_json::json!({"seen": {"alt": "a cat"}}),
        },
    )
    .await
    .expect("store pick");
    HeraldPost::record_delivery(
        &client,
        later,
        SLOT,
        HeraldNetwork::Bluesky,
        "https://bsky.app/1",
    )
    .await
    .expect("record delivery");
    HeraldPost::record_delivery(
        &client,
        later,
        SLOT,
        HeraldNetwork::Bluesky,
        "https://bsky.app/2",
    )
    .await
    .expect("record delivery again");
    HeraldPost::mark_failed(&client, later, SLOT)
        .await
        .expect("fail");
    let retry = HeraldPost::claim(&client, later, SLOT, fresh(), window(), MAX_ATTEMPTS)
        .await
        .expect("claim")
        .expect("a failed day under the cap is claimed again");
    assert_eq!(retry, first, "the idempotency key survives a retry");
    assert_ne!(key, first, "each day has its own key");
    assert_eq!(
        HeraldPost::stored_pick(&client, later, SLOT)
            .await
            .expect("read"),
        Some(HeraldPick {
            picture: serde_json::json!({}),
            look: serde_json::json!({"seen": {"alt": "a cat"}}),
        }),
        "the look survives the retry with the picture"
    );
    assert_eq!(
        HeraldPost::deliveries(&client, later, SLOT)
            .await
            .expect("read"),
        vec![HeraldDelivery {
            network: HeraldNetwork::Bluesky,
            url: "https://bsky.app/1".to_string(),
        }]
    );
    assert_eq!(
        HeraldPost::posted_on(&client, later, SLOT)
            .await
            .expect("read"),
        None,
        "a delivery is not a posted day"
    );
}

/// The day's two posts are separate claims with separate keys, picks and
/// deliveries: one settled leaves the other free, and neither reads the other.
#[tokio::test]
async fn each_slot_has_its_own_claim() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");

    let art = HeraldPost::claim(
        &client,
        DAY,
        HeraldSlot::Art,
        fresh(),
        window(),
        MAX_ATTEMPTS,
    )
    .await
    .expect("claim")
    .expect("art wins");
    post(&client, DAY, HeraldKind::Piece, "piece:1").await;

    assert!(
        !HeraldPost::is_finished(&client, DAY, HeraldSlot::Words, window(), MAX_ATTEMPTS)
            .await
            .expect("read"),
        "art settled says nothing about words"
    );
    assert_eq!(
        HeraldPost::stored_pick(&client, DAY, HeraldSlot::Words)
            .await
            .expect("read"),
        None
    );
    let words = HeraldPost::claim(
        &client,
        DAY,
        HeraldSlot::Words,
        fresh(),
        window(),
        MAX_ATTEMPTS,
    )
    .await
    .expect("claim")
    .expect("words wins the same day");
    assert_ne!(words, art, "each post has its own idempotency key");
    assert_eq!(
        HeraldPost::deliveries(&client, DAY, HeraldSlot::Words)
            .await
            .expect("read"),
        Vec::new()
    );
    assert_eq!(
        HeraldPost::deliveries(&client, DAY, HeraldSlot::Art)
            .await
            .expect("read")
            .len(),
        1
    );
}

/// A kind's picks read back oldest first from the bound on, whether the post
/// settled or not; a slot with no pick and other kinds stay out.
#[tokio::test]
async fn picked_since_reads_a_kinds_picks_from_the_bound_on() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let older = DAY - Duration::days(70);
    let settled = DAY - Duration::days(30);

    for (day, kind, subject) in [
        (older, HeraldKind::Project, "project:old"),
        (settled, HeraldKind::Project, "project:settled"),
        (DAY - Duration::days(29), HeraldKind::Paper, "paper:x"),
    ] {
        HeraldPost::claim(&client, day, SLOT, fresh(), window(), MAX_ATTEMPTS)
            .await
            .expect("claim")
            .expect("wins");
        post(&client, day, kind, subject).await;
    }
    // Picked and failed: it may be on one network, so it counts.
    HeraldPost::claim(&client, DAY, SLOT, fresh(), window(), MAX_ATTEMPTS)
        .await
        .expect("claim")
        .expect("wins");
    HeraldPost::store_pick(
        &client,
        DAY,
        SLOT,
        HeraldKind::Project,
        "project:failed",
        &HeraldPick {
            picture: serde_json::json!({}),
            look: serde_json::json!("drawn"),
        },
    )
    .await
    .expect("store pick");
    HeraldPost::mark_failed(&client, DAY, SLOT)
        .await
        .expect("fail");
    // Claimed with no pick yet: nothing to count.
    HeraldPost::claim(
        &client,
        DAY,
        HeraldSlot::Words,
        fresh(),
        window(),
        MAX_ATTEMPTS,
    )
    .await
    .expect("claim")
    .expect("wins");

    assert_eq!(
        HeraldPost::picked_since(&client, HeraldKind::Project, DAY - Duration::days(60))
            .await
            .expect("read"),
        vec![
            HeraldPicked {
                day: settled,
                subject: "project:settled".to_string(),
            },
            HeraldPicked {
                day: DAY,
                subject: "project:failed".to_string(),
            },
        ]
    );
}

/// A failed day is retried up to the cap and then rests; a claim nobody
/// settled is taken over once stale; a skipped day is finished.
#[tokio::test]
async fn failures_retry_to_the_cap_and_stale_claims_are_taken_over() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");

    for attempt in 1..=MAX_ATTEMPTS {
        HeraldPost::claim(&client, DAY, SLOT, fresh(), window(), MAX_ATTEMPTS)
            .await
            .expect("claim")
            .unwrap_or_else(|| panic!("attempt {attempt} claims"));
        HeraldPost::mark_failed(&client, DAY, SLOT)
            .await
            .expect("fail");
    }
    assert_eq!(
        HeraldPost::claim(&client, DAY, SLOT, fresh(), window(), MAX_ATTEMPTS)
            .await
            .expect("claim"),
        None,
        "at the cap the day rests"
    );
    assert!(
        HeraldPost::is_finished(&client, DAY, SLOT, window(), MAX_ATTEMPTS)
            .await
            .expect("read")
    );

    let stuck = DAY + Duration::days(1);
    HeraldPost::claim(&client, stuck, SLOT, fresh(), window(), MAX_ATTEMPTS)
        .await
        .expect("claim")
        .expect("wins");
    assert_eq!(
        HeraldPost::claim(&client, stuck, SLOT, fresh(), window(), MAX_ATTEMPTS)
            .await
            .expect("claim"),
        None,
        "a live claim is held"
    );
    let stale_bound_in_the_future = Utc::now() + Duration::minutes(1);
    HeraldPost::claim(
        &client,
        stuck,
        SLOT,
        stale_bound_in_the_future,
        window(),
        MAX_ATTEMPTS,
    )
    .await
    .expect("claim")
    .expect("a stale claim is taken over");

    HeraldPost::finish_skipped(&client, stuck, SLOT)
        .await
        .expect("skip");
    assert!(
        HeraldPost::is_finished(&client, stuck, SLOT, window(), MAX_ATTEMPTS)
            .await
            .expect("read"),
        "a skipped day is finished"
    );
    assert_eq!(
        HeraldPost::posted_on(&client, stuck, SLOT)
            .await
            .expect("read"),
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

/// A claim older than the window is never taken again, whatever the
/// attempt count says: a stale `posting` row past it rests as failed.
#[tokio::test]
async fn a_claim_past_the_window_rests_as_failed() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");

    HeraldPost::claim(&client, DAY, SLOT, fresh(), window(), MAX_ATTEMPTS)
        .await
        .expect("claim")
        .expect("wins");
    // The replica died an hour ago, mid-call.
    client
        .execute(
            "UPDATE herald_posts SET claimed_at = $2, first_claimed_at = $2 WHERE day = $1",
            &[&DAY, &(Utc::now() - Duration::hours(1))],
        )
        .await
        .expect("age the claim");
    assert_eq!(
        HeraldPost::claim(&client, DAY, SLOT, fresh(), window(), MAX_ATTEMPTS)
            .await
            .expect("claim"),
        None,
        "past the window the day is not taken over"
    );
    assert!(
        HeraldPost::is_finished(&client, DAY, SLOT, window(), MAX_ATTEMPTS)
            .await
            .expect("read")
    );
    let day = HeraldPost::day(&client, DAY, SLOT)
        .await
        .expect("read")
        .expect("row");
    assert_eq!(day.status, HeraldDayStatus::Failed);
    assert_eq!(day.attempts, 1, "no attempt was spent");
}

/// `/herald on` settles a day nobody claimed as skipped, never to be
/// claimed; a day with a row is left alone.
#[tokio::test]
async fn an_unclaimed_day_is_settled_unposted() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");

    assert!(
        HeraldPost::settle_unclaimed(&client, DAY, SLOT)
            .await
            .expect("settle")
    );
    assert!(
        !HeraldPost::settle_unclaimed(&client, DAY, SLOT)
            .await
            .expect("settle again"),
        "a row is left alone"
    );
    assert!(
        HeraldPost::is_finished(&client, DAY, SLOT, window(), MAX_ATTEMPTS)
            .await
            .expect("read")
    );
    assert_eq!(
        HeraldPost::claim(&client, DAY, SLOT, fresh(), window(), MAX_ATTEMPTS)
            .await
            .expect("claim"),
        None
    );
    let day = HeraldPost::day(&client, DAY, SLOT)
        .await
        .expect("read")
        .expect("row");
    assert_eq!(day.status, HeraldDayStatus::Skipped);
    assert_eq!(day.attempts, 0);

    let claimed = DAY + Duration::days(1);
    HeraldPost::claim(&client, claimed, SLOT, fresh(), window(), MAX_ATTEMPTS)
        .await
        .expect("claim")
        .expect("wins");
    assert!(
        !HeraldPost::settle_unclaimed(&client, claimed, SLOT)
            .await
            .expect("settle"),
        "a claimed day is not touched"
    );
}
