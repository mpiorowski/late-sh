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
