use crate::{
    models::{
        article::{Article, ArticleParams},
        article_body::{ArticleBody, BodyContent, BodyStatus, ReaderLine, ReaderSpan},
    },
    test_utils::{create_test_user, test_db},
};
use chrono::{Duration, Utc};
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

fn feed_status(
    feed: &[crate::models::article::FeedArticle],
    id: Uuid,
) -> (BodyStatus, Option<u16>) {
    let row = feed
        .iter()
        .find(|row| row.article.id == id)
        .expect("article in feed");
    (row.body, row.reading_minutes)
}

/// A story with no body row is pending on the feed; a stored body reads
/// back line for line and puts its reading time on the feed; an
/// unavailable one carries no time.
#[tokio::test]
async fn bodies_round_trip_and_ride_the_feed_as_status_only() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let sharer = create_test_user(&test_db.db, "article-body-sharer").await;
    let readable = share(&client, sharer.id, "https://example.com/body-ready").await;
    let video = share(&client, sharer.id, "https://youtu.be/body-video").await;

    let feed = Article::list_feed(&client, 20).await.expect("feed");
    assert_eq!(feed_status(&feed, readable.id), (BodyStatus::Pending, None));
    assert_eq!(
        ArticleBody::find(&client, readable.id).await.expect("find"),
        None
    );

    let lines = vec![
        ReaderLine::Heading {
            level: 1,
            text: "Why it stops".to_string(),
        },
        ReaderLine::Paragraph {
            spans: vec![
                ReaderSpan::Text("See ".to_string()),
                ReaderSpan::Link {
                    text: "the docs".to_string(),
                    href: "https://example.com/docs".to_string(),
                },
            ],
        },
        ReaderLine::Code {
            lines: vec!["LISTEN news;".to_string()],
        },
        ReaderLine::Rule,
    ];
    ArticleBody::store_ready(&client, readable.id, "example.com", 441, &lines)
        .await
        .expect("store ready");
    ArticleBody::store_unavailable(&client, video.id, "youtu.be")
        .await
        .expect("store unavailable");

    let body = ArticleBody::find(&client, readable.id)
        .await
        .expect("find")
        .expect("ready body");
    assert_eq!(body.domain, "example.com");
    assert_eq!(
        body.content,
        BodyContent::Ready {
            word_count: 441,
            lines,
        }
    );

    let feed = Article::list_feed(&client, 20).await.expect("feed");
    assert_eq!(
        feed_status(&feed, readable.id),
        (BodyStatus::Ready, Some(3))
    );
    assert_eq!(
        feed_status(&feed, video.id),
        (BodyStatus::Unavailable, None)
    );
}

/// Retention keeps the row and drops the text, so the card reads `link`
/// rather than waiting on a fetch that will never come. A fresh body is
/// left alone.
#[tokio::test]
async fn retention_retires_old_bodies_to_unavailable() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let sharer = create_test_user(&test_db.db, "article-body-retention").await;
    let old = share(&client, sharer.id, "https://example.com/body-old").await;
    let fresh = share(&client, sharer.id, "https://example.com/body-fresh").await;
    let lines = vec![ReaderLine::Paragraph {
        spans: vec![ReaderSpan::Text("words".to_string())],
    }];
    for id in [old.id, fresh.id] {
        ArticleBody::store_ready(&client, id, "example.com", 1, &lines)
            .await
            .expect("store ready");
    }
    client
        .execute(
            "UPDATE article_bodies SET fetched_at = current_timestamp - interval '31 days'
             WHERE article_id = $1",
            &[&old.id],
        )
        .await
        .expect("age the old body");

    let retired = ArticleBody::retire_fetched_before(&client, Utc::now() - Duration::days(30))
        .await
        .expect("retire");

    assert_eq!(retired, 1);
    let old_body = ArticleBody::find(&client, old.id)
        .await
        .expect("find old")
        .expect("old row kept");
    assert_eq!(old_body.content, BodyContent::Unavailable);
    assert_eq!(
        ArticleBody::find(&client, fresh.id)
            .await
            .expect("find fresh")
            .expect("fresh row")
            .status(),
        BodyStatus::Ready
    );
}

/// A status no build wrote is an error, not a guess.
#[tokio::test]
async fn an_unknown_stored_status_fails_to_parse() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let sharer = create_test_user(&test_db.db, "article-body-unknown").await;
    let article = share(&client, sharer.id, "https://example.com/body-unknown").await;
    client
        .execute(
            "INSERT INTO article_bodies (article_id, status, domain, word_count, lines)
             VALUES ($1, 'archived', 'example.com', 0, '[]'::jsonb)",
            &[&article.id],
        )
        .await
        .expect("insert unknown status");

    assert!(ArticleBody::find(&client, article.id).await.is_err());
    assert!(Article::list_feed(&client, 20).await.is_err());
}
