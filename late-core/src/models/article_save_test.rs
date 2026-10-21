use crate::{
    models::{
        article::{Article, ArticleParams},
        article_save::{ArticleSave, NEWS_SAVED_LIMIT, SaveOutcome},
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

async fn shelf(client: &tokio_postgres::Client, user_id: Uuid) -> Vec<Uuid> {
    ArticleSave::list_for_user(client, user_id)
        .await
        .expect("shelf")
        .into_iter()
        .map(|row| row.article.id)
        .collect()
}

/// A save is the reader's alone, saving twice is one save, the shelf lists
/// newest save first, and an unsave takes only that story off.
#[tokio::test]
async fn saves_are_per_reader_newest_first_and_an_unsave_takes_one_off() {
    let test_db = test_db().await;
    let mut client = test_db.db.get().await.expect("db client");
    let sharer = create_test_user(&test_db.db, "article-save-sharer").await;
    let reader = create_test_user(&test_db.db, "article-save-reader").await;
    let other = create_test_user(&test_db.db, "article-save-other").await;
    let first = share(&client, sharer.id, "https://example.com/save-1").await;
    let second = share(&client, sharer.id, "https://example.com/save-2").await;

    assert_eq!(
        ArticleSave::save(&mut client, reader.id, first.id)
            .await
            .expect("save first"),
        SaveOutcome::Saved
    );
    assert_eq!(
        ArticleSave::save(&mut client, reader.id, first.id)
            .await
            .expect("save first again"),
        SaveOutcome::AlreadySaved
    );
    assert_eq!(
        ArticleSave::save(&mut client, reader.id, second.id)
            .await
            .expect("save second"),
        SaveOutcome::Saved
    );
    assert_eq!(shelf(&client, reader.id).await, vec![second.id, first.id]);
    assert_eq!(shelf(&client, other.id).await, Vec::<Uuid>::new());

    ArticleSave::unsave(&client, reader.id, second.id)
        .await
        .expect("unsave second");
    ArticleSave::unsave(&client, reader.id, second.id)
        .await
        .expect("unsave second again");
    assert_eq!(shelf(&client, reader.id).await, vec![first.id]);
}

/// The shelf holds `NEWS_SAVED_LIMIT` stories: the next save is refused
/// and writes nothing, and letting one go makes room again.
#[tokio::test]
async fn the_shelf_refuses_a_save_past_its_limit() {
    let test_db = test_db().await;
    let mut client = test_db.db.get().await.expect("db client");
    let sharer = create_test_user(&test_db.db, "article-save-cap-sharer").await;
    let reader = create_test_user(&test_db.db, "article-save-cap-reader").await;
    let mut shared = Vec::new();
    for n in 0..=NEWS_SAVED_LIMIT {
        shared.push(share(&client, sharer.id, &format!("https://example.com/cap-{n}")).await);
    }
    for article in shared.iter().take(NEWS_SAVED_LIMIT as usize) {
        assert_eq!(
            ArticleSave::save(&mut client, reader.id, article.id)
                .await
                .expect("save"),
            SaveOutcome::Saved
        );
    }
    let extra = shared.last().expect("one past the limit");
    assert_eq!(
        ArticleSave::save(&mut client, reader.id, extra.id)
            .await
            .expect("save past the limit"),
        SaveOutcome::Full
    );
    assert_eq!(
        shelf(&client, reader.id).await.len(),
        NEWS_SAVED_LIMIT as usize
    );

    ArticleSave::unsave(&client, reader.id, shared[0].id)
        .await
        .expect("unsave");
    assert_eq!(
        ArticleSave::save(&mut client, reader.id, extra.id)
            .await
            .expect("save with room"),
        SaveOutcome::Saved
    );
    assert_eq!(shelf(&client, reader.id).await[0], extra.id);
}

/// A deleted story leaves every shelf it was on.
#[tokio::test]
async fn a_deleted_story_leaves_the_shelf() {
    let test_db = test_db().await;
    let mut client = test_db.db.get().await.expect("db client");
    let sharer = create_test_user(&test_db.db, "article-save-delete-sharer").await;
    let reader = create_test_user(&test_db.db, "article-save-delete-reader").await;
    let article = share(&client, sharer.id, "https://example.com/save-delete").await;
    ArticleSave::save(&mut client, reader.id, article.id)
        .await
        .expect("save");
    Article::delete(&client, article.id).await.expect("delete");
    assert_eq!(shelf(&client, reader.id).await, Vec::<Uuid>::new());
}
