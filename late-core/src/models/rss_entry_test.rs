use uuid::Uuid;

use crate::{
    models::{
        rss_entry::{RssEntry, RssEntryParams},
        rss_feed::RssFeed,
    },
    test_utils::{create_test_user, test_db},
};

fn entry(feed: &RssFeed, guid: &str, title: &str) -> RssEntryParams {
    RssEntryParams {
        feed_id: feed.id,
        user_id: feed.user_id,
        guid: guid.to_string(),
        url: format!("https://example.com/{}/{guid}", feed.id),
        title: title.to_string(),
        summary: "summary".to_string(),
        published_at: None,
        shared_at: None,
        dismissed_at: None,
    }
}

async fn guids(client: &tokio_postgres::Client, feed_id: Uuid) -> Vec<String> {
    client
        .query(
            "SELECT guid FROM rss_entries WHERE feed_id = $1 ORDER BY created, id",
            &[&feed_id],
        )
        .await
        .expect("read entries")
        .into_iter()
        .map(|row| row.get("guid"))
        .collect()
}

async fn updated_at(
    client: &tokio_postgres::Client,
    feed_id: Uuid,
    guid: &str,
) -> (String, chrono::DateTime<chrono::Utc>) {
    let row = client
        .query_one(
            "SELECT title, updated FROM rss_entries WHERE feed_id = $1 AND guid = $2",
            &[&feed_id, &guid],
        )
        .await
        .expect("read entry");
    (row.get("title"), row.get("updated"))
}

#[tokio::test]
async fn a_poll_only_writes_an_entry_that_changed() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = create_test_user(&test_db.db, "rss-upsert").await;
    let feed = RssFeed::create_for_user(&client, user.id, "https://example.com/feed.xml")
        .await
        .expect("feed");

    let inserted = RssEntry::upsert_for_feed(&client, entry(&feed, "a", "First"))
        .await
        .expect("insert");
    assert!(inserted.is_some(), "a new guid is a new entry");
    let (_, written) = updated_at(&client, feed.id, "a").await;

    let same = RssEntry::upsert_for_feed(&client, entry(&feed, "a", "First"))
        .await
        .expect("same");
    assert!(same.is_none(), "a known guid is not a new entry");
    assert_eq!(
        updated_at(&client, feed.id, "a").await,
        ("First".to_string(), written),
        "an unchanged entry is not rewritten"
    );

    let healed = RssEntry::upsert_for_feed(&client, entry(&feed, "a", "Second"))
        .await
        .expect("heal");
    assert!(healed.is_none(), "a known guid is not a new entry");
    let (title, rewritten) = updated_at(&client, feed.id, "a").await;
    assert_eq!(title, "Second");
    assert!(rewritten > written, "a changed entry is rewritten");
}

#[tokio::test]
async fn pruning_drains_each_feed_down_to_its_newest_in_batches() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = create_test_user(&test_db.db, "rss-prune").await;
    let busy = RssFeed::create_for_user(&client, user.id, "https://example.com/busy.xml")
        .await
        .expect("busy feed");
    let quiet = RssFeed::create_for_user(&client, user.id, "https://example.com/quiet.xml")
        .await
        .expect("quiet feed");
    for guid in ["1", "2", "3", "4"] {
        RssEntry::upsert_for_feed(&client, entry(&busy, guid, "Item"))
            .await
            .expect("busy entry");
    }
    RssEntry::upsert_for_feed(&client, entry(&quiet, "only", "Item"))
        .await
        .expect("quiet entry");

    let deleted = RssEntry::prune_past_keep(&client, 2, 1).await.expect("run");
    assert_eq!(deleted, 1, "a batch caps one run");
    let deleted = RssEntry::prune_past_keep(&client, 2, 1).await.expect("run");
    assert_eq!(deleted, 1);
    let deleted = RssEntry::prune_past_keep(&client, 2, 1).await.expect("run");
    assert_eq!(deleted, 0, "nothing is left past the keep");

    assert_eq!(guids(&client, busy.id).await, ["3", "4"]);
    assert_eq!(guids(&client, quiet.id).await, ["only"]);
}
