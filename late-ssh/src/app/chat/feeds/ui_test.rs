use std::cell::Cell;

use chrono::Utc;
use late_core::models::rss_entry::{RssEntry, RssEntryView};
use ratatui::{Terminal, backend::TestBackend};
use uuid::Uuid;

use super::{FeedListView, draw_feed_list};

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

/// Draws sharing one kept offset, four entries to a window of 28 rows: the
/// window holds while the selection moves inside it and scrolls only when
/// the entry past the selection would leave it, in both directions.
#[test]
fn the_window_holds_until_the_next_entry_would_leave_it() {
    let entries: Vec<_> = (0..10).map(entry).collect();
    let scroll = Cell::new(0);
    let top_title = |selected_index| {
        let view = FeedListView {
            entries: &entries,
            selected_index,
            has_feeds: true,
            marker_read_at: None,
            scroll: &scroll,
        };
        let mut terminal = Terminal::new(TestBackend::new(60, 28)).expect("terminal");
        terminal
            .draw(|frame| draw_feed_list(frame, frame.area(), &view))
            .expect("draw");
        let buffer = terminal.backend().buffer();
        (0..buffer.area.width)
            .map(|x| buffer[(x, 0)].symbol())
            .collect::<String>()
            .trim_end()
            .to_string()
    };

    // The first fill lands on the newest entry, at the bottom.
    assert_eq!(top_title(9), "● Entry 6");
    assert_eq!(top_title(8), "● Entry 6", "8 moves inside the window");
    assert_eq!(top_title(7), "● Entry 6", "6, the entry above, is in view");
    assert_eq!(top_title(6), "● Entry 5", "5 would have left the window");
    // And back down: still until the entry below would leave it.
    assert_eq!(top_title(7), "● Entry 5");
    assert_eq!(top_title(8), "● Entry 6");
    // The ends stop at the ends.
    assert_eq!(top_title(0), "● Entry 0");
    assert_eq!(top_title(9), "● Entry 6");
}
