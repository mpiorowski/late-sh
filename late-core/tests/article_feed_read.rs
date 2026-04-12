use late_core::{
    models::{
        article::{Article, ArticleParams},
        article_feed_read::ArticleFeedRead,
    },
    test_utils::{create_test_user, test_db},
};
use tokio::time::{Duration, sleep};

#[tokio::test]
async fn article_feed_unread_uses_timestamp_cursor() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let author = create_test_user(&test_db.db, "article-author").await;
    let reader = create_test_user(&test_db.db, "article-reader").await;

    Article::create_by_user_id(
        &client,
        author.id,
        ArticleParams {
            user_id: author.id,
            url: "https://example.com/one".to_string(),
            title: "One".to_string(),
            summary: "First".to_string(),
            ascii_art: "...".to_string(),
        },
    )
    .await
    .expect("create article one");

    Article::create_by_user_id(
        &client,
        author.id,
        ArticleParams {
            user_id: author.id,
            url: "https://example.com/two".to_string(),
            title: "Two".to_string(),
            summary: "Second".to_string(),
            ascii_art: "+++".to_string(),
        },
    )
    .await
    .expect("create article two");

    let unread_before = ArticleFeedRead::unread_count_for_user(&client, reader.id)
        .await
        .expect("count unread before");
    assert_eq!(unread_before, 2);

    ArticleFeedRead::mark_read_now(&client, reader.id)
        .await
        .expect("mark read");

    let unread_after = ArticleFeedRead::unread_count_for_user(&client, reader.id)
        .await
        .expect("count unread after mark read");
    assert_eq!(unread_after, 0);

    sleep(Duration::from_millis(5)).await;

    Article::create_by_user_id(
        &client,
        author.id,
        ArticleParams {
            user_id: author.id,
            url: "https://example.com/three".to_string(),
            title: "Three".to_string(),
            summary: "Third".to_string(),
            ascii_art: "***".to_string(),
        },
    )
    .await
    .expect("create article three");

    let unread_after_new = ArticleFeedRead::unread_count_for_user(&client, reader.id)
        .await
        .expect("count unread after new article");
    assert_eq!(unread_after_new, 1);
}
