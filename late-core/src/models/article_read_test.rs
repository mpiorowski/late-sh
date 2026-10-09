use crate::{
    models::{
        article::{Article, ArticleParams},
        article_feed_read::ArticleFeedRead,
        article_read::ArticleRead,
    },
    test_utils::{create_test_user, test_db},
};
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

/// A read is the reader's alone, opening it twice is one read, and a cursor
/// that moved past it deletes it while a read after the cursor stays.
#[tokio::test]
async fn article_reads_are_per_reader_and_the_cursor_deletes_what_it_covers() {
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

    ArticleFeedRead::mark_read_now(&client, reader.id)
        .await
        .expect("move cursor");
    let cursor = ArticleFeedRead::last_read_at(&client, reader.id)
        .await
        .expect("cursor")
        .expect("cursor row");
    ArticleRead::mark_read(&client, reader.id, second.id)
        .await
        .expect("mark second");
    ArticleRead::delete_covered(&client, reader.id, cursor)
        .await
        .expect("delete covered");

    assert_eq!(
        ArticleRead::article_ids_for_user(&client, reader.id)
            .await
            .expect("reader reads after the cursor"),
        vec![second.id]
    );
}
