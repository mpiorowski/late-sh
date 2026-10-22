use crate::{
    models::{
        article::{Article, ArticleParams},
        article_body::{ArticleBody, BodyContent, BodyStatus, ReaderLine, ReaderSpan},
        article_save::ArticleSave,
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

fn feed_status(feed: &[crate::models::article::FeedArticle], id: Uuid) -> BodyStatus {
    feed.iter()
        .find(|row| row.article.id == id)
        .expect("article in feed")
        .body
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
    assert_eq!(feed_status(&feed, readable.id), BodyStatus::Pending);
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
    ArticleBody::store_ready(&client, readable.id, 441, &lines)
        .await
        .expect("store ready");
    ArticleBody::store_unavailable(&client, video.id)
        .await
        .expect("store unavailable");

    let body = ArticleBody::find(&client, readable.id)
        .await
        .expect("find")
        .expect("ready body");
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
        BodyStatus::Ready { reading_minutes: 3 }
    );
    assert_eq!(feed_status(&feed, video.id), BodyStatus::Unavailable);
}

/// Retention keeps the row and drops the text, so the card reads `link`
/// rather than waiting on a fetch that will never come. A fresh body is
/// left alone, and so is an old one somebody saved.
#[tokio::test]
async fn retention_retires_old_bodies_to_unavailable() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let sharer = create_test_user(&test_db.db, "article-body-retention").await;
    let old = share(&client, sharer.id, "https://example.com/body-old").await;
    let fresh = share(&client, sharer.id, "https://example.com/body-fresh").await;
    let kept = share(&client, sharer.id, "https://example.com/body-kept").await;
    let lines = vec![ReaderLine::Paragraph {
        spans: vec![ReaderSpan::Text("words".to_string())],
    }];
    for id in [old.id, fresh.id, kept.id] {
        ArticleBody::store_ready(&client, id, 1, &lines)
            .await
            .expect("store ready");
    }
    client
        .execute(
            "UPDATE article_bodies SET fetched_at = current_timestamp - interval '31 days'
             WHERE article_id = ANY($1)",
            &[&vec![old.id, kept.id]],
        )
        .await
        .expect("age the old bodies");
    // A story on a reader's shelf keeps its text past retention.
    let reader = create_test_user(&test_db.db, "article-body-retention-reader").await;
    let mut saving_client = test_db.db.get().await.expect("db client");
    ArticleSave::save(&mut saving_client, reader.id, kept.id)
        .await
        .expect("save the kept story");

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
        BodyStatus::Ready { reading_minutes: 1 }
    );
    assert_eq!(
        ArticleBody::find(&client, kept.id)
            .await
            .expect("find kept")
            .expect("kept row")
            .status(),
        BodyStatus::Ready { reading_minutes: 1 }
    );
}

/// The schema holds the body contract, so no write path can land a row the
/// readers would choke on: one bad row would fail the whole feed read on
/// every replica.
#[tokio::test]
async fn the_schema_refuses_rows_outside_the_body_contract() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let sharer = create_test_user(&test_db.db, "article-body-contract").await;
    let article = share(&client, sharer.id, "https://example.com/body-contract").await;

    for (status, word_count, lines) in [
        ("archived", 0, "[]"),
        ("ready", 0, "[]"),
        ("unavailable", 3, "[]"),
        ("unavailable", 0, r#"[{"kind": "rule"}]"#),
        ("ready", -1, "[]"),
    ] {
        let inserted = client
            .execute(
                "INSERT INTO article_bodies (article_id, status, word_count, lines)
                 VALUES ($1, $2, $3, $4::text::jsonb)",
                &[&article.id, &status, &word_count, &lines],
            )
            .await;
        assert!(
            inserted.is_err(),
            "{status} with {word_count} words and {lines} must be refused"
        );
    }
    assert_eq!(
        ArticleBody::find(&client, article.id).await.expect("find"),
        None
    );
}

/// A story past the share budget with no body row had its fetch cut short
/// (a restart, an abort, a store that failed). The sweep settles it as
/// `unavailable`, so the card reads `link` instead of fetching forever; a
/// story inside the budget and a story with a body are left alone.
#[tokio::test]
async fn pending_bodies_past_the_share_budget_are_settled_unavailable() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let sharer = create_test_user(&test_db.db, "article-body-settle").await;
    let stuck = share(&client, sharer.id, "https://example.com/body-stuck").await;
    let fresh = share(&client, sharer.id, "https://example.com/body-fresh").await;
    let read = share(&client, sharer.id, "https://example.com/body-read").await;
    let lines = vec![ReaderLine::Paragraph {
        spans: vec![ReaderSpan::Text("words".to_string())],
    }];
    ArticleBody::store_ready(&client, read.id, 1, &lines)
        .await
        .expect("store ready");
    for id in [stuck.id, read.id] {
        client
            .execute(
                "UPDATE articles SET created = current_timestamp - interval '10 minutes'
                 WHERE id = $1",
                &[&id],
            )
            .await
            .expect("age the article");
    }

    let settled = ArticleBody::settle_pending_before(&client, Utc::now() - Duration::minutes(6))
        .await
        .expect("settle");

    assert_eq!(settled, 1);
    let stuck_body = ArticleBody::find(&client, stuck.id)
        .await
        .expect("find stuck")
        .expect("settled row");
    assert_eq!(stuck_body.content, BodyContent::Unavailable);
    assert_eq!(
        ArticleBody::find(&client, fresh.id)
            .await
            .expect("find fresh"),
        None,
        "a story inside the budget is still pending"
    );
    assert_eq!(
        ArticleBody::find(&client, read.id)
            .await
            .expect("find read")
            .expect("ready row")
            .status(),
        BodyStatus::Ready { reading_minutes: 1 }
    );
}
