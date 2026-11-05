use chrono::{Duration, Utc};
use uuid::Uuid;

use crate::{
    models::rss_feed::RssFeed,
    test_utils::{create_test_user, test_db},
};

async fn last_seen_days_ago(client: &tokio_postgres::Client, user_id: Uuid, days: i64) {
    client
        .execute(
            "UPDATE users SET last_seen = $1 WHERE id = $2",
            &[&(Utc::now() - Duration::days(days)), &user_id],
        )
        .await
        .expect("age the owner");
}

#[tokio::test]
async fn a_poll_skips_owners_who_have_gone_and_serves_them_first_on_return() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let present = create_test_user(&test_db.db, "rss-present").await;
    let absent = create_test_user(&test_db.db, "rss-absent").await;
    let present_feed =
        RssFeed::create_for_user(&client, present.id, "https://example.com/present.xml")
            .await
            .expect("present feed");
    let absent_feed =
        RssFeed::create_for_user(&client, absent.id, "https://example.com/absent.xml")
            .await
            .expect("absent feed");
    RssFeed::record_success(&client, present_feed.id, "Present")
        .await
        .expect("present was checked");
    last_seen_days_ago(&client, absent.id, 15).await;
    let cutoff = Utc::now() - Duration::days(14);

    let polled = RssFeed::list_for_poll(&client, cutoff, 10)
        .await
        .expect("list");
    let ids: Vec<Uuid> = polled.iter().map(|feed| feed.id).collect();
    assert_eq!(ids, [present_feed.id]);

    last_seen_days_ago(&client, absent.id, 0).await;

    let polled = RssFeed::list_for_poll(&client, cutoff, 10)
        .await
        .expect("list");
    let ids: Vec<Uuid> = polled.iter().map(|feed| feed.id).collect();
    assert_eq!(ids, [absent_feed.id, present_feed.id]);
}
