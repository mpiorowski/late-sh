use chrono::Utc;
use late_core::models::rss_entry::{RssEntry, RssEntryView};
use uuid::Uuid;

use super::State;
use crate::app::ai::svc::AiService;
use crate::app::chat::feeds::svc::FeedService;
use crate::app::chat::news::svc::ArticleService;
use crate::app::chat::notifications::svc::NotificationService;
use crate::app::chat::svc::ChatService;
use crate::test_helpers::new_test_db;

fn entry(id: u128) -> RssEntryView {
    let now = Utc::now();
    RssEntryView {
        entry: RssEntry {
            id: Uuid::from_u128(id),
            created: now,
            updated: now,
            feed_id: Uuid::from_u128(1_000),
            user_id: Uuid::from_u128(2_000),
            guid: id.to_string(),
            url: format!("https://example.com/{id}"),
            title: format!("Entry {id}"),
            summary: String::new(),
            published_at: None,
            shared_at: None,
            dismissed_at: None,
        },
        feed_title: "Blog".to_string(),
        feed_url: "https://example.com/feed.xml".to_string(),
    }
}

fn ids(state: &State) -> Vec<u128> {
    state
        .all_entries()
        .iter()
        .map(|item| item.entry.id.as_u128())
        .collect()
}

fn selected(state: &State) -> u128 {
    state.all_entries()[state.selected_index()].entry.id.as_u128()
}

/// The newest-first snapshot shows oldest first, the newest entry at the
/// bottom where the first fill lands; a refresh keeps the cursor on its
/// entry as new ones arrive below and old ones leave above, and a dismissed
/// entry hands its row to the one that took its place.
#[tokio::test]
async fn entries_run_oldest_to_newest_and_the_cursor_holds_its_entry() {
    let test_db = new_test_db().await;
    let db = test_db.db.clone();
    let chat = ChatService::new(db.clone(), NotificationService::new(db.clone()));
    let articles = ArticleService::new(db.clone(), AiService::new(false, None), chat);
    let mut state = State::new(FeedService::new(db), articles, Uuid::from_u128(2_000));

    state.set_entries(vec![entry(3), entry(2), entry(1)]);
    assert_eq!(ids(&state), vec![1, 2, 3]);
    assert_eq!(selected(&state), 3, "the first fill lands on the newest");

    state.move_selection(-1);
    state.set_entries(vec![entry(4), entry(3), entry(2)]);
    assert_eq!(ids(&state), vec![2, 3, 4]);
    assert_eq!(selected(&state), 2, "the cursor stays on its entry");

    state.set_entries(vec![entry(4), entry(3)]);
    assert_eq!(selected(&state), 3, "a dismissed entry's row goes to the next");
}
