use crate::models::article::NEWS_FEED_LIMIT;
use crate::{
    models::{
        article::{Article, ArticleParams},
        article_read::ArticleRead,
    },
    test_utils::{create_test_user, test_db},
};
use std::collections::HashSet;
use uuid::Uuid;

async fn share(client: &tokio_postgres::Client, user_id: Uuid, url: &str) -> Article {
    Article::create_by_user_id(
        client,
        user_id,
        ArticleParams {
            user_id,
            url: url.to_string(),
            title: "Title".to_string(),
            summary: "Summary".to_string(),
            ascii_art: "A".to_string(),
        },
    )
    .await
    .expect("create article")
}

/// A read is the reader's alone, opening it twice is one read, and reading
/// the feed adds every snapshot article without touching another reader.
#[tokio::test]
async fn article_reads_are_per_reader_and_the_feed_read_covers_the_snapshot() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let sharer = create_test_user(&test_db.db, "article-read-sharer").await;
    let reader = create_test_user(&test_db.db, "article-read-reader").await;
    let other = create_test_user(&test_db.db, "article-read-other").await;
    let first = share(&client, sharer.id, "https://example.com/read-1").await;
    let second = share(&client, sharer.id, "https://example.com/read-2").await;

    ArticleRead::mark_read(&client, reader.id, first.id)
        .await
        .expect("mark first");
    ArticleRead::mark_read(&client, reader.id, first.id)
        .await
        .expect("mark first again");
    assert_eq!(
        ArticleRead::article_ids_for_user(&client, reader.id)
            .await
            .expect("reader reads"),
        vec![first.id]
    );
    assert_eq!(
        ArticleRead::article_ids_for_user(&client, other.id)
            .await
            .expect("other reads"),
        Vec::<Uuid>::new()
    );

    ArticleRead::mark_feed_read(&client, reader.id)
        .await
        .expect("read the feed");
    let reads: HashSet<Uuid> = ArticleRead::article_ids_for_user(&client, reader.id)
        .await
        .expect("reader reads after the feed")
        .into_iter()
        .collect();
    assert!(reads.contains(&first.id) && reads.contains(&second.id));
    assert_eq!(
        ArticleRead::article_ids_for_user(&client, other.id)
            .await
            .expect("other reads after the feed"),
        Vec::<Uuid>::new()
    );
}

/// Only the snapshot's articles ride a load: a read of an article past the
/// newest [`NEWS_FEED_LIMIT`] is kept but never loaded, and the feed read
/// covers exactly the snapshot.
#[tokio::test]
async fn a_load_carries_only_the_snapshots_articles() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let sharer = create_test_user(&test_db.db, "article-window-sharer").await;
    let reader = create_test_user(&test_db.db, "article-window-reader").await;
    let oldest = share(&client, sharer.id, "https://example.com/window-0").await;
    for n in 1..=NEWS_FEED_LIMIT {
        share(
            &client,
            sharer.id,
            &format!("https://example.com/window-{n}"),
        )
        .await;
    }

    ArticleRead::mark_read(&client, reader.id, oldest.id)
        .await
        .expect("mark the oldest");
    assert_eq!(
        ArticleRead::article_ids_for_user(&client, reader.id)
            .await
            .expect("reads"),
        Vec::<Uuid>::new(),
        "an article past the snapshot is not loaded"
    );

    ArticleRead::mark_feed_read(&client, reader.id)
        .await
        .expect("read the feed");
    let reads = ArticleRead::article_ids_for_user(&client, reader.id)
        .await
        .expect("reads after the feed");
    assert_eq!(reads.len(), NEWS_FEED_LIMIT as usize);
    assert!(!reads.contains(&oldest.id));
}
