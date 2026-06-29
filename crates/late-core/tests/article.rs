use late_core::{
    models::{
        article::{Article, ArticleParams},
        user::{User, UserParams},
    },
    test_utils::test_db,
};

#[tokio::test]
async fn test_list_recent_articles() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");

    // Create a user to own the articles
    let user = User::create(
        &client,
        UserParams {
            fingerprint: "article-test-user".to_string(),
            username: "article_tester".to_string(),
            settings: serde_json::json!({}),
        },
    )
    .await
    .expect("create user");

    // Insert articles
    let article1 = Article::create_by_user_id(
        &client,
        user.id,
        ArticleParams {
            user_id: user.id,
            url: "https://example.com/1".to_string(),
            title: "First Article".to_string(),
            summary: "This is the first article".to_string(),
            ascii_art: "A".to_string(),
        },
    )
    .await
    .expect("create article 1");

    // Pause briefly to ensure timestamps differ if precision is tight
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    let article2 = Article::create_by_user_id(
        &client,
        user.id,
        ArticleParams {
            user_id: user.id,
            url: "https://example.com/2".to_string(),
            title: "Second Article".to_string(),
            summary: "This is the second article".to_string(),
            ascii_art: "B".to_string(),
        },
    )
    .await
    .expect("create article 2");

    // Fetch recent, limit 1
    let recent = Article::list_recent(&client, 1).await.expect("list recent");
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].id, article2.id); // Should be the newest one

    // Fetch recent, limit 10
    let recent_all = Article::list_recent(&client, 10)
        .await
        .expect("list recent");
    assert_eq!(recent_all.len(), 2);
    assert_eq!(recent_all[0].id, article2.id);
    assert_eq!(recent_all[1].id, article1.id);
}
