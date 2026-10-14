use super::{
    Reads, State, clamp_index, has_fresh_unread_from_others, is_unread, is_unread_at, move_index,
    news_unread_label, unread_in_snapshot,
};
use crate::app::ai::svc::AiService;
use crate::app::chat::news::svc::ArticleService;
use crate::test_helpers::{new_test_db, wait_until};
use chrono::{DateTime, Duration, Utc};
use late_core::models::article::{
    Article, ArticleEvent, ArticleFeedItem, ArticleParams, NEWS_FEED_LIMIT,
};
use late_core::test_utils::create_test_user;
use std::collections::HashSet;
use uuid::Uuid;

const READER: Uuid = Uuid::from_u128(100);
const OTHER: Uuid = Uuid::from_u128(200);

fn at(minutes: i64) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-09-23T12:00:00Z")
        .unwrap()
        .with_timezone(&Utc)
        + Duration::minutes(minutes)
}

fn read(ids: &[u128]) -> HashSet<Uuid> {
    ids.iter().map(|id| Uuid::from_u128(*id)).collect()
}

fn article(id: u128, author: Uuid, created_minutes: i64) -> ArticleFeedItem {
    let created = at(created_minutes);
    ArticleFeedItem {
        article: Article {
            id: Uuid::from_u128(id),
            created,
            updated: created,
            user_id: author,
            url: format!("https://example.com/{id}"),
            title: format!("Article {id}"),
            summary: "Summary".to_string(),
            ascii_art: "...".to_string(),
        },
        author_username: "author".to_string(),
    }
}

/// Unread is a snapshot article without a read, your own shares included;
/// a read is one article and no other.
#[test]
fn unread_counts_the_snapshot_articles_without_a_read_including_your_own() {
    let feed = [
        article(3, READER, 30),
        article(2, OTHER, 20),
        article(1, OTHER, 10),
    ];

    assert_eq!(unread_in_snapshot(&feed, &read(&[])), 3);
    assert_eq!(unread_in_snapshot(&feed, &read(&[1])), 2);
    assert_eq!(unread_in_snapshot(&feed, &read(&[1, 2, 3])), 0);
    assert!(!is_unread(&feed[2], &read(&[1])));
    assert!(is_unread(&feed[1], &read(&[1])));
}

/// Nothing is unread until the reads have loaded, so no mark ever guesses.
#[test]
fn nothing_is_unread_while_the_reads_load() {
    let item = article(1, OTHER, 10);

    assert!(!is_unread_at(&item, &Reads::Loading));
    assert!(is_unread_at(&item, &Reads::Loaded(read(&[]))));
    assert!(!is_unread_at(&item, &Reads::Loaded(read(&[1]))));
}

/// Two writes' loads landing in the wrong order: the fuller one first, then
/// one taken before the second write. Reads only grow, so the session
/// merges and never takes a read back.
#[tokio::test]
async fn a_load_landing_out_of_order_never_takes_a_read_back() {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let sharer = create_test_user(&test_db.db, "merge-sharer").await;
    let reader = create_test_user(&test_db.db, "merge-reader").await;
    let share = |url: &str| ArticleParams {
        user_id: sharer.id,
        url: url.to_string(),
        title: "Title".to_string(),
        summary: "Summary".to_string(),
        ascii_art: "...".to_string(),
    };
    let first = Article::create_by_user_id(&client, sharer.id, share("https://example.com/m1"))
        .await
        .expect("create first");
    let second = Article::create_by_user_id(&client, sharer.id, share("https://example.com/m2"))
        .await
        .expect("create second");
    let service = ArticleService::new(test_db.db.clone(), AiService::new(false, None));
    let mut state = State::new(service.clone(), reader.id, false);
    wait_until(
        || {
            state.tick();
            std::future::ready(
                state.all_articles().len() == 2 && matches!(state.reads(), Reads::Loaded(_)),
            )
        },
        "the snapshot and the reads load",
    )
    .await;
    assert_eq!(state.unread_count(), 2);

    service.publish_event(ArticleEvent::ReadsLoaded {
        user_id: reader.id,
        read_article_ids: vec![first.id, second.id],
    });
    service.publish_event(ArticleEvent::ReadsLoaded {
        user_id: reader.id,
        read_article_ids: vec![first.id],
    });
    state.tick();

    assert_eq!(state.unread_count(), 0);
    let second_item = state
        .all_articles()
        .iter()
        .find(|item| item.article.id == second.id)
        .expect("second article in the snapshot");
    assert!(!is_unread_at(second_item, state.reads()));
}

#[test]
fn a_full_unread_snapshot_reads_as_a_floor() {
    assert_eq!(news_unread_label(3), "3");
    assert_eq!(news_unread_label(NEWS_FEED_LIMIT - 1), "19");
    assert_eq!(news_unread_label(NEWS_FEED_LIMIT), "20+");
}

#[test]
fn a_new_unread_article_from_someone_else_is_announced() {
    let previous = [article(1, OTHER, 10)];
    let next = [article(2, OTHER, 20), article(1, OTHER, 10)];

    assert!(has_fresh_unread_from_others(
        &previous,
        &next,
        &read(&[1]),
        READER
    ));
}

#[test]
fn your_own_share_is_not_announced_to_you() {
    let previous = [article(1, OTHER, 10)];
    let next = [article(2, READER, 20), article(1, OTHER, 10)];

    assert!(!has_fresh_unread_from_others(
        &previous,
        &next,
        &read(&[1]),
        READER
    ));
}

#[test]
fn a_refresh_that_only_reorders_or_drops_is_not_announced() {
    let previous = [article(2, OTHER, 20), article(1, OTHER, 10)];
    let next = [article(2, OTHER, 20)];

    assert!(!has_fresh_unread_from_others(
        &previous,
        &next,
        &read(&[]),
        READER
    ));
}

/// The snapshot is capped, so deleting one of its articles pulls an older
/// one in. That article's id was never seen, but it is not news.
#[test]
fn an_older_article_backfilling_a_delete_is_not_announced() {
    let previous = [article(3, OTHER, 30), article(2, OTHER, 20)];
    let next = [article(3, OTHER, 30), article(1, OTHER, 10)];

    assert!(!has_fresh_unread_from_others(
        &previous,
        &next,
        &read(&[]),
        READER
    ));
}

#[test]
fn the_first_snapshot_a_session_sees_is_not_announced() {
    let next = [article(2, OTHER, 20), article(1, OTHER, 10)];

    assert!(!has_fresh_unread_from_others(
        &[],
        &next,
        &read(&[]),
        READER
    ));
}

#[test]
fn clamp_index_handles_empty_list() {
    assert_eq!(clamp_index(4, 0), 0);
}

#[test]
fn clamp_index_caps_to_last_item() {
    assert_eq!(clamp_index(9, 3), 2);
}

#[test]
fn move_index_moves_within_bounds() {
    assert_eq!(move_index(2, -1, 5), 1);
    assert_eq!(move_index(2, 2, 5), 4);
}

#[test]
fn move_index_clamps_at_edges() {
    assert_eq!(move_index(0, -1, 5), 0);
    assert_eq!(move_index(4, 1, 5), 4);
}

#[test]
fn move_index_returns_zero_for_empty_list() {
    assert_eq!(move_index(0, 1, 0), 0);
    assert_eq!(move_index(3, -1, 0), 0);
}

#[test]
fn clamp_index_passes_through_when_within_bounds() {
    assert_eq!(clamp_index(1, 5), 1);
}
