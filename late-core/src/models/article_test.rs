use crate::{
    models::{
        article::{
            Article, ArticleParams, NEWS_SHARE_MAX_PAID_PER_DAY, NEWS_SHARE_REWARD_CHIPS,
            NewsShareReward,
        },
        chips::{INITIAL_CHIP_BALANCE, UserChips},
        user::{User, UserParams},
    },
    test_utils::{bump_created_past_now, create_test_user, test_db},
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

    // Both inserts can land on the same microsecond; separate them explicitly
    // so recency ordering is decisive.
    bump_created_past_now(&client, "articles", "id = $1", &[&article2.id]).await;

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

/// Publishing pays the sharer, and pays them exactly once for a link.
/// Deleting a story frees its URL to be shared again, so without the ledger
/// check one player could share, delete, and re-share the same link forever.
#[tokio::test]
async fn create_shared_pays_the_sharer_once_per_url() {
    let test_db = test_db().await;
    let mut client = test_db.db.get().await.expect("db client");
    let user = create_test_user(&test_db.db, "news-reward-sharer").await;
    UserChips::ensure(&client, user.id)
        .await
        .expect("chips row");

    let url = "https://example.com/paid-once";
    let (article, reward) =
        Article::create_shared(&mut client, user.id, share_params(user.id, url))
            .await
            .expect("first share");
    assert_eq!(reward, NewsShareReward::Paid);
    assert_eq!(
        balance(&client, user.id).await,
        INITIAL_CHIP_BALANCE + NEWS_SHARE_REWARD_CHIPS
    );

    Article::delete(&client, article.id)
        .await
        .expect("delete article");

    let (_, second_reward) =
        Article::create_shared(&mut client, user.id, share_params(user.id, url))
            .await
            .expect("second share");
    assert_eq!(second_reward, NewsShareReward::RepeatUrl);
    assert_eq!(
        balance(&client, user.id).await,
        INITIAL_CHIP_BALANCE + NEWS_SHARE_REWARD_CHIPS
    );
}

/// The cap is per person, not per link. `articles.url` is unique, so a
/// second player can only reach a link once the first one's story is gone,
/// and when they do the share still pays them.
#[tokio::test]
async fn create_shared_pays_a_second_sharer_of_a_freed_url() {
    let test_db = test_db().await;
    let mut client = test_db.db.get().await.expect("db client");
    let first = create_test_user(&test_db.db, "news-reward-first").await;
    let second = create_test_user(&test_db.db, "news-reward-second").await;
    UserChips::ensure(&client, second.id)
        .await
        .expect("chips row");

    let url = "https://example.com/shared-twice";
    let (article, _) = Article::create_shared(&mut client, first.id, share_params(first.id, url))
        .await
        .expect("first share");
    Article::delete(&client, article.id)
        .await
        .expect("delete article");

    let (_, reward) = Article::create_shared(&mut client, second.id, share_params(second.id, url))
        .await
        .expect("second share");

    assert_eq!(reward, NewsShareReward::Paid);
    assert_eq!(
        balance(&client, second.id).await,
        INITIAL_CHIP_BALANCE + NEWS_SHARE_REWARD_CHIPS
    );
}

/// `s` on an RSS entry is one keypress, so the day cap is what stops an
/// inbox from being a chip printer. Shares past it still publish and pay
/// nothing; the cap rolls with the UTC date, so yesterday's paid shares do
/// not count against today.
#[tokio::test]
async fn create_shared_pays_at_most_the_daily_cap() {
    let test_db = test_db().await;
    let mut client = test_db.db.get().await.expect("db client");
    let user = create_test_user(&test_db.db, "news-reward-capped").await;
    UserChips::ensure(&client, user.id)
        .await
        .expect("chips row");

    for n in 0..NEWS_SHARE_MAX_PAID_PER_DAY {
        let url = format!("https://example.com/capped-{n}");
        let (_, reward) = Article::create_shared(&mut client, user.id, share_params(user.id, &url))
            .await
            .expect("share inside the cap");
        assert_eq!(reward, NewsShareReward::Paid, "share {n} inside the cap");
    }
    let paid = INITIAL_CHIP_BALANCE + NEWS_SHARE_REWARD_CHIPS * NEWS_SHARE_MAX_PAID_PER_DAY;
    assert_eq!(balance(&client, user.id).await, paid);

    let (article, reward) = Article::create_shared(
        &mut client,
        user.id,
        share_params(user.id, "https://example.com/capped-over"),
    )
    .await
    .expect("share past the cap");
    assert_eq!(reward, NewsShareReward::DailyCapReached);
    assert_eq!(balance(&client, user.id).await, paid);
    assert!(
        Article::find_by_url(&client, "https://example.com/capped-over")
            .await
            .expect("lookup")
            .is_some(),
        "a capped share still publishes"
    );
    Article::delete(&client, article.id)
        .await
        .expect("delete article");

    // Yesterday's rewards are yesterday's: the same URL, refused by the cap a
    // moment ago, pays once today's ledger rows age out of the day.
    let aged = client
        .execute(
            "UPDATE chip_ledger SET created_at = created_at - interval '1 day'
             WHERE user_id = $1 AND reason = 'news_shared'",
            &[&user.id],
        )
        .await
        .expect("age the ledger");
    assert_eq!(aged as i64, NEWS_SHARE_MAX_PAID_PER_DAY);

    let (_, reward) = Article::create_shared(
        &mut client,
        user.id,
        share_params(user.id, "https://example.com/capped-over"),
    )
    .await
    .expect("share the next day");
    assert_eq!(reward, NewsShareReward::Paid);
    assert_eq!(
        balance(&client, user.id).await,
        paid + NEWS_SHARE_REWARD_CHIPS
    );
}

fn share_params(user_id: uuid::Uuid, url: &str) -> ArticleParams {
    ArticleParams {
        user_id,
        url: url.to_string(),
        title: "Paid Article".to_string(),
        summary: "A shared link".to_string(),
        ascii_art: "A".to_string(),
    }
}

async fn balance(client: &tokio_postgres::Client, user_id: uuid::Uuid) -> i64 {
    UserChips::find(client, user_id)
        .await
        .expect("chips lookup")
        .expect("chips row")
        .balance
}
