use uuid::Uuid;

use crate::{
    models::{
        rss_entry::{RssEntry, RssEntryParams},
        rss_feed::RssFeed,
    },
    test_utils::{create_test_user, test_db},
};

const KEEP: i64 = 100;

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

fn dated(feed: &RssFeed, guid: &str, days_ago: i64) -> RssEntryParams {
    RssEntryParams {
        published_at: Some(chrono::Utc::now() - chrono::Duration::days(days_ago)),
        ..entry(feed, guid, "Item")
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

    let inserted = RssEntry::upsert_for_feed(&client, entry(&feed, "a", "First"), KEEP)
        .await
        .expect("insert");
    assert!(inserted.is_some(), "a new guid is a new entry");
    let (_, written) = updated_at(&client, feed.id, "a").await;

    let same = RssEntry::upsert_for_feed(&client, entry(&feed, "a", "First"), KEEP)
        .await
        .expect("same");
    assert!(same.is_none(), "a known guid is not a new entry");
    assert_eq!(
        updated_at(&client, feed.id, "a").await,
        ("First".to_string(), written),
        "an unchanged entry is not rewritten"
    );

    let healed = RssEntry::upsert_for_feed(&client, entry(&feed, "a", "Second"), KEEP)
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
        RssEntry::upsert_for_feed(&client, entry(&busy, guid, "Item"), KEEP)
            .await
            .expect("busy entry");
    }
    RssEntry::upsert_for_feed(&client, entry(&quiet, "only", "Item"), KEEP)
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

/// A feed that keeps listing an item past the keep would otherwise hand it
/// back on every poll after the prune took it: unread again, its dismissal
/// gone, a new-entries notice each time.
#[tokio::test]
async fn a_pruned_item_the_feed_still_lists_does_not_come_back() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = create_test_user(&test_db.db, "rss-pruned-stays").await;
    let feed = RssFeed::create_for_user(&client, user.id, "https://example.com/sticky.xml")
        .await
        .expect("feed");
    for (guid, days_ago) in [("old", 3), ("mid", 2), ("new", 1)] {
        RssEntry::upsert_for_feed(&client, dated(&feed, guid, days_ago), 2)
            .await
            .expect("entry");
    }
    let deleted = RssEntry::prune_past_keep(&client, 2, 10)
        .await
        .expect("prune");
    assert_eq!(deleted, 1, "the oldest by publish date goes");
    assert_eq!(guids(&client, feed.id).await, ["mid", "new"]);

    let back = RssEntry::upsert_for_feed(&client, dated(&feed, "old", 3), 2)
        .await
        .expect("sticky item");
    assert!(
        back.is_none(),
        "an item older than every kept row stays out"
    );
    assert_eq!(guids(&client, feed.id).await, ["mid", "new"]);

    let fresh = RssEntry::upsert_for_feed(&client, dated(&feed, "newer", 0), 2)
        .await
        .expect("new item");
    assert!(fresh.is_some(), "a newer item still lands");
    let undated = RssEntry::upsert_for_feed(&client, entry(&feed, "undated", "Item"), 2)
        .await
        .expect("undated item");
    assert!(
        undated.is_some(),
        "an item with no publish date counts as now"
    );
}
