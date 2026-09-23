use crate::{
    models::article_feed_read::ArticleFeedRead,
    test_utils::{create_test_user, test_db},
};

/// News counts its unread badge in the session against this cursor, so the
/// cursor is the whole contract: no row reads as `None` (everything unread),
/// marking read stores a time, and the new-user seed never moves a cursor
/// that already exists.
#[tokio::test]
async fn article_read_cursor_moves_on_mark_read_and_survives_the_seed() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let reader = create_test_user(&test_db.db, "article-reader").await;

    let before = ArticleFeedRead::last_read_at(&client, reader.id)
        .await
        .expect("cursor before");
    assert_eq!(before, None);

    ArticleFeedRead::mark_read_now(&client, reader.id)
        .await
        .expect("mark read");
    let marked = ArticleFeedRead::last_read_at(&client, reader.id)
        .await
        .expect("cursor after mark read");
    assert!(marked.is_some());

    ArticleFeedRead::seed_read_for_new_user(&client, reader.id)
        .await
        .expect("seed");
    let after_seed = ArticleFeedRead::last_read_at(&client, reader.id)
        .await
        .expect("cursor after seed");
    assert_eq!(after_seed, marked);
}
