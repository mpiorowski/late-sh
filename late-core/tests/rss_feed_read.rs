use chrono::{Duration as ChronoDuration, Utc};
use late_core::{
    models::{
        rss_entry::{RssEntry, RssEntryParams},
        rss_feed::RssFeed,
        rss_feed_read::RssFeedRead,
    },
    test_utils::{create_test_user, test_db},
};
use tokio::time::{Duration, sleep};

#[tokio::test]
async fn rss_feed_unread_uses_timestamp_cursor() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let reader = create_test_user(&test_db.db, "rss-reader").await;
    let feed = RssFeed::create_for_user(&client, reader.id, "https://example.com/feed.xml")
        .await
        .expect("create feed");

    for (guid, url, title) in [
        ("one", "https://example.com/one", "One"),
        ("two", "https://example.com/two", "Two"),
    ] {
        RssEntry::upsert_for_feed(
            &client,
            RssEntryParams {
                feed_id: feed.id,
                user_id: reader.id,
                guid: guid.to_string(),
                url: url.to_string(),
                title: title.to_string(),
                summary: String::new(),
                published_at: None,
                shared_at: None,
                dismissed_at: None,
            },
        )
        .await
        .expect("create rss entry");
    }

    let unread_before = RssFeedRead::unread_count_for_user(&client, reader.id)
        .await
        .expect("count unread before");
    assert_eq!(unread_before, 2);

    RssFeedRead::mark_read_now(&client, reader.id)
        .await
        .expect("mark read");

    let unread_after = RssFeedRead::unread_count_for_user(&client, reader.id)
        .await
        .expect("count unread after mark read");
    assert_eq!(unread_after, 0);

    sleep(Duration::from_millis(5)).await;

    RssEntry::upsert_for_feed(
        &client,
        RssEntryParams {
            feed_id: feed.id,
            user_id: reader.id,
            guid: "three".to_string(),
            url: "https://example.com/three".to_string(),
            title: "Three".to_string(),
            summary: String::new(),
            published_at: None,
            shared_at: None,
            dismissed_at: None,
        },
    )
    .await
    .expect("create rss entry after read");

    let unread_after_new = RssFeedRead::unread_count_for_user(&client, reader.id)
        .await
        .expect("count unread after new entry");
    assert_eq!(unread_after_new, 1);
}

#[tokio::test]
async fn rss_visible_entries_cap_per_feed_so_quiet_feeds_survive() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let reader = create_test_user(&test_db.db, "rss-fairness").await;
    let loud = RssFeed::create_for_user(&client, reader.id, "https://loud.example/feed.xml")
        .await
        .expect("create loud feed");
    let quiet = RssFeed::create_for_user(&client, reader.id, "https://quiet.example/feed.xml")
        .await
        .expect("create quiet feed");

    // Loud feed: 25 fresh entries, newer than the quiet feed's single entry.
    for i in 0..25 {
        RssEntry::upsert_for_feed(
            &client,
            RssEntryParams {
                feed_id: loud.id,
                user_id: reader.id,
                guid: format!("loud-{i}"),
                url: format!("https://loud.example/{i}"),
                title: format!("Loud {i}"),
                summary: String::new(),
                published_at: Some(Utc::now() - ChronoDuration::minutes(i)),
                shared_at: None,
                dismissed_at: None,
            },
        )
        .await
        .expect("create loud entry");
    }
    RssEntry::upsert_for_feed(
        &client,
        RssEntryParams {
            feed_id: quiet.id,
            user_id: reader.id,
            guid: "quiet-1".to_string(),
            url: "https://quiet.example/1".to_string(),
            title: "Quiet weekly digest".to_string(),
            summary: String::new(),
            published_at: Some(Utc::now() - ChronoDuration::days(6)),
            shared_at: None,
            dismissed_at: None,
        },
    )
    .await
    .expect("create quiet entry");

    // A flat limit of 20 would be all loud entries; the per-feed cap of 10
    // must keep the quiet feed visible.
    let visible = RssEntry::list_visible_for_user(&client, reader.id, 20, 10)
        .await
        .expect("list visible");
    let loud_count = visible
        .iter()
        .filter(|view| view.entry.feed_id == loud.id)
        .count();
    let quiet_count = visible
        .iter()
        .filter(|view| view.entry.feed_id == quiet.id)
        .count();
    assert_eq!(loud_count, 10);
    assert_eq!(quiet_count, 1);
    // Ordering stays newest-first across feeds.
    assert_eq!(
        visible.last().expect("entries").entry.feed_id,
        quiet.id,
        "oldest visible entry should be the quiet feed's digest"
    );
}
