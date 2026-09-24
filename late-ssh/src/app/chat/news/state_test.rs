use super::{
    clamp_index, has_fresh_unread_from_others, move_index, news_unread_label, unread_in_snapshot,
};
use chrono::{DateTime, Duration, Utc};
use late_core::models::article::{Article, ArticleFeedItem, NEWS_FEED_LIMIT};
use uuid::Uuid;

const READER: Uuid = Uuid::from_u128(100);
const OTHER: Uuid = Uuid::from_u128(200);

fn at(minutes: i64) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-09-23T12:00:00Z")
        .unwrap()
        .with_timezone(&Utc)
        + Duration::minutes(minutes)
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

#[test]
fn unread_counts_articles_newer_than_the_cursor_including_your_own() {
    let feed = [
        article(3, READER, 30),
        article(2, OTHER, 20),
        article(1, OTHER, 10),
    ];

    assert_eq!(unread_in_snapshot(&feed, Some(at(15))), 2);
    assert_eq!(unread_in_snapshot(&feed, Some(at(30))), 0);
}

#[test]
fn a_missing_cursor_row_means_every_article_is_unread() {
    let feed = [article(2, OTHER, 20), article(1, OTHER, 10)];

    assert_eq!(unread_in_snapshot(&feed, None), 2);
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
        Some(at(15)),
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
        Some(at(15)),
        READER
    ));
}

#[test]
fn a_refresh_that_only_reorders_or_drops_is_not_announced() {
    let previous = [article(2, OTHER, 20), article(1, OTHER, 10)];
    let next = [article(2, OTHER, 20)];

    assert!(!has_fresh_unread_from_others(
        &previous, &next, None, READER
    ));
}

/// The snapshot is capped, so deleting one of its articles pulls an older
/// one in. That article's id was never seen, but it is not news.
#[test]
fn an_older_article_backfilling_a_delete_is_not_announced() {
    let previous = [article(3, OTHER, 30), article(2, OTHER, 20)];
    let next = [article(3, OTHER, 30), article(1, OTHER, 10)];

    assert!(!has_fresh_unread_from_others(
        &previous, &next, None, READER
    ));
}

#[test]
fn the_first_snapshot_a_session_sees_is_not_announced() {
    let next = [article(2, OTHER, 20), article(1, OTHER, 10)];

    assert!(!has_fresh_unread_from_others(&[], &next, None, READER));
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
