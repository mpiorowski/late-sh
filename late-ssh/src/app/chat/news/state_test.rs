use super::{
    Reads, State, clamp_index, display_order, has_fresh_unread_from_others, is_unread,
    is_unread_at, move_index, news_unread_label, unread_in_snapshot,
};
use crate::app::ai::svc::AiService;
use crate::app::chat::news::svc::ArticleService;
use crate::test_helpers::{new_test_db, wait_until};
use chrono::{DateTime, Duration, Utc};
use late_core::models::article::{
    Article, ArticleEvent, ArticleFeedItem, ArticleParams, NEWS_FEED_LIMIT,
};
use late_core::models::article_read::{ArticleReadRow, WatchPosition};
use late_core::test_utils::create_test_user;
use std::collections::{HashMap, HashSet};
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

/// A loaded read with no video position.
fn read_row(article_id: Uuid) -> ArticleReadRow {
    ArticleReadRow {
        article_id,
        watch_position: None,
    }
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
        body: late_core::models::article_body::BodyStatus::Pending,
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
/// merges and never takes a read back. Video positions move, so the newest
/// load replaces them: one without the position is a clear that landed.
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

    let halfway = WatchPosition {
        position_s: 1394,
        duration_s: Some(2910),
    };
    service.publish_event(ArticleEvent::ReadsLoaded {
        user_id: reader.id,
        reads: vec![
            read_row(first.id),
            ArticleReadRow {
                article_id: second.id,
                watch_position: Some(halfway),
            },
        ],
    });
    state.tick();
    assert_eq!(
        state.watch_positions(),
        &HashMap::from([(second.id, halfway)])
    );

    service.publish_event(ArticleEvent::ReadsLoaded {
        user_id: reader.id,
        reads: vec![read_row(first.id)],
    });
    state.tick();

    assert_eq!(state.unread_count(), 0);
    assert_eq!(
        state.watch_positions(),
        &HashMap::new(),
        "a load without the position is the clear that landed"
    );
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

/// The shelf leads, newest save first, then the feed less what is on the
/// shelf: a saved story appears once, and the feed's copy (the fresher
/// body) is the one shown. Mine-only filters both parts.
#[test]
fn the_display_puts_the_shelf_over_the_feed_once_each() {
    let feed = vec![
        article(3, OTHER, 3),
        article(2, READER, 2),
        article(1, OTHER, 1),
    ];
    let mut saved_two = article(2, READER, 2);
    saved_two.body = late_core::models::article_body::BodyStatus::Unavailable;
    let saved = vec![saved_two, article(9, OTHER, -100)];

    let (rows, saved_len) = display_order(&saved, &feed, None);
    let ids: Vec<u128> = rows.iter().map(|item| item.article.id.as_u128()).collect();
    assert_eq!(ids, vec![2, 9, 3, 1]);
    assert_eq!(saved_len, 2);
    assert_eq!(rows[0].body, feed[1].body, "the feed's copy is shown");

    let (rows, saved_len) = display_order(&saved, &feed, Some(READER));
    let ids: Vec<u128> = rows.iter().map(|item| item.article.id.as_u128()).collect();
    assert_eq!(ids, vec![2]);
    assert_eq!(saved_len, 1);

    let (rows, saved_len) = display_order(&[], &feed, None);
    assert_eq!(rows.len(), 3);
    assert_eq!(saved_len, 0);
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

/// A body answer reaches only the session that asked. With the same user
/// on a second session (a phone beside the laptop), the phone opening the
/// same story leaves the laptop's reader where it was: its text, and how
/// far down it had scrolled.
#[tokio::test]
async fn a_body_answer_reaches_only_the_session_that_asked() {
    use crate::app::chat::news::reader::state::ReaderBody;
    use crate::app::common::scroll_view::ScrollViewport;
    use late_core::models::article_body::{ArticleBody, ReaderLine, ReaderSpan};
    use ratatui::layout::Rect;

    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let sharer = create_test_user(&test_db.db, "two-sessions-sharer").await;
    let reader = create_test_user(&test_db.db, "two-sessions-reader").await;
    let story = Article::create_by_user_id(
        &client,
        sharer.id,
        ArticleParams {
            user_id: sharer.id,
            url: "https://example.com/two-sessions".to_string(),
            title: "Title".to_string(),
            summary: "Summary".to_string(),
            ascii_art: "...".to_string(),
        },
    )
    .await
    .expect("create story");
    let lines = vec![ReaderLine::Paragraph {
        spans: vec![ReaderSpan::Text("words".to_string())],
    }];
    ArticleBody::store_ready(&client, story.id, 1, &lines)
        .await
        .expect("store ready");
    let service = ArticleService::new(test_db.db.clone(), AiService::new(false, None));
    let mut laptop = State::new(service.clone(), reader.id, false);
    let mut phone = State::new(service.clone(), reader.id, false);
    wait_until(
        || {
            laptop.tick();
            phone.tick();
            std::future::ready(laptop.all_articles().len() == 1 && phone.all_articles().len() == 1)
        },
        "both sessions load the snapshot",
    )
    .await;
    let is_ready = |state: &State| {
        matches!(
            state.reader().map(|modal| modal.body()),
            Some(ReaderBody::Ready { .. })
        )
    };

    assert!(laptop.open_reader(story.id));
    wait_until(
        || {
            laptop.tick();
            std::future::ready(is_ready(&laptop))
        },
        "the laptop's reader gets its text",
    )
    .await;
    let view = laptop.reader().expect("laptop reader").view();
    view.set_viewport(ScrollViewport {
        body: Rect::new(0, 0, 40, 10),
        content_height: 50,
        ..ScrollViewport::default()
    });
    view.scroll(7);

    assert!(phone.open_reader(story.id));
    wait_until(
        || {
            laptop.tick();
            phone.tick();
            std::future::ready(is_ready(&phone))
        },
        "the phone's reader gets its text",
    )
    .await;
    laptop.tick();

    let laptop_reader = laptop.reader().expect("laptop reader still open");
    assert!(matches!(laptop_reader.body(), ReaderBody::Ready { .. }));
    assert_eq!(
        laptop_reader.view().offset(),
        7,
        "the phone's answer must not reach the laptop"
    );
}
