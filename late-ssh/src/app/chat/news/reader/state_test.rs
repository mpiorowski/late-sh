use chrono::Utc;
use late_core::models::article::{Article, ArticleFeedItem};
use late_core::models::article_body::{
    ArticleBody, BodyContent, BodyStatus, ReaderLine, ReaderSpan,
};
use ratatui::layout::Rect;
use uuid::Uuid;

use super::{ReaderBody, ReaderModal, ReaderRow};
use crate::app::common::scroll_view::ScrollViewport;

fn story(body: BodyStatus) -> ArticleFeedItem {
    let now = Utc::now();
    ArticleFeedItem {
        article: Article {
            id: Uuid::from_u128(7),
            created: now,
            updated: now,
            lounge_message_id: None,
            user_id: Uuid::from_u128(1),
            url: "https://www.example.com/listen-notify".to_string(),
            title: "Why LISTEN/NOTIFY stops scaling".to_string(),
            summary: "• one listener per process\n• bursts collapse".to_string(),
            ascii_art: "####\n####".to_string(),
        },
        author_username: "mat".to_string(),
        body,
    }
}

fn text(text: &str) -> ReaderSpan {
    ReaderSpan::Text(text.to_string())
}

fn link(text: &str, href: &str) -> ReaderSpan {
    ReaderSpan::Link {
        text: text.to_string(),
        href: href.to_string(),
    }
}

fn ready(lines: Vec<ReaderLine>) -> Option<ArticleBody> {
    Some(ArticleBody {
        article_id: Uuid::from_u128(7),
        content: BodyContent::Ready {
            word_count: 12,
            lines,
        },
        fetched_at: Utc::now(),
    })
}

fn plain(rows: &[ReaderRow]) -> Vec<String> {
    rows.iter()
        .map(|row| row.iter().map(|segment| segment.text.as_str()).collect())
        .collect()
}

/// A ready body opens loading, then lays out as the article: the header,
/// each line kind in its form, links numbered in the order the text first
/// reaches them (a repeat keeps its number), and the Links list after. The
/// lines landing replace what was laid out while loading.
#[test]
fn a_ready_body_lays_out_with_links_numbered_in_first_appearance_order() {
    let mut reader = ReaderModal::open(&story(BodyStatus::Ready { reading_minutes: 1 }));
    assert_eq!(*reader.body(), ReaderBody::Loading { reading_minutes: 1 });
    assert_eq!(
        plain(&reader.lay_out(30)).last().map(String::as_str),
        Some("Loading the article…")
    );

    reader.apply_body(ready(vec![
        ReaderLine::Heading {
            level: 2,
            text: "The setup".to_string(),
        },
        ReaderLine::Paragraph {
            spans: vec![
                text("Read "),
                link("the docs", "https://b.example/docs"),
                text(" then "),
                link("the bench", "https://a.example/bench"),
            ],
        },
        ReaderLine::Quote {
            spans: vec![text("It holds. "), link("docs", "https://b.example/docs")],
        },
        ReaderLine::Code {
            lines: vec!["LISTEN news;".to_string()],
        },
        ReaderLine::ListItem {
            spans: vec![text("one connection per replica, every channel on it")],
        },
        ReaderLine::Rule,
    ]));

    assert_eq!(reader.link(1), Some("https://b.example/docs"));
    assert_eq!(reader.link(2), Some("https://a.example/bench"));
    assert_eq!(reader.link(3), None);
    let rows = plain(&reader.lay_out(30));
    assert_eq!(
        rows[..2],
        ["Why LISTEN/NOTIFY stops".to_string(), "scaling".to_string(),]
    );
    assert_eq!(rows[2], "example.com · 1 min read ·");
    assert!(rows[3].starts_with("shared by @mat"));
    let rule = rows
        .iter()
        .position(|row| *row == "─".repeat(30))
        .expect("the header rule");
    assert_eq!(
        rows[rule..],
        [
            "─".repeat(30),
            String::new(),
            "THE SETUP".to_string(),
            "Read the docs [1] then the".to_string(),
            "bench [2]".to_string(),
            String::new(),
            "│ It holds. docs [1]".to_string(),
            String::new(),
            "│ LISTEN news;".to_string(),
            String::new(),
            "• one connection per replica,".to_string(),
            "  every channel on it".to_string(),
            "─".repeat(24),
            String::new(),
            "LINKS".to_string(),
            "[1] https://b.example/docs".to_string(),
            "[2] https://a.example/bench".to_string(),
        ]
    );
}

/// A story whose fetch has not landed shows its summary under the art and
/// says it is fetching; one with no text to read points at the web.
#[test]
fn a_pending_or_unavailable_body_shows_the_summary_and_one_status_line() {
    let reader = ReaderModal::open(&story(BodyStatus::Pending));
    let rows = plain(&reader.lay_out(60));
    assert_eq!(rows[..3], ["####", "####", ""]);
    assert_eq!(rows[3], "Why LISTEN/NOTIFY stops scaling");
    assert!(rows[4].starts_with("example.com · shared by @mat"));
    assert_eq!(
        rows[5..],
        [
            "─".repeat(60),
            String::new(),
            "• one listener per process".to_string(),
            "• bursts collapse".to_string(),
            String::new(),
            "Fetching the article…".to_string(),
        ]
    );

    let mut reader = ReaderModal::open(&story(BodyStatus::Ready { reading_minutes: 1 }));
    reader.apply_body(Some(ArticleBody {
        article_id: Uuid::from_u128(7),
        content: BodyContent::Unavailable,
        fetched_at: Utc::now(),
    }));
    let rows = plain(&reader.lay_out(60));
    assert_eq!(
        rows.last().map(String::as_str),
        Some("Read on the web: https://www.example.com/listen-notify")
    );
    assert!(!reader.has_links());
}

/// A video is watched, not read: pending or not, its fallback names the
/// watch link, and the footer offers `w`.
#[test]
fn a_video_shows_the_watch_line_and_offers_w() {
    for body in [BodyStatus::Pending, BodyStatus::Unavailable] {
        let mut item = story(body);
        item.article.url = "https://www.youtube.com/watch?v=dQw4w9WgXcQ".to_string();
        let reader = ReaderModal::open(&item);
        assert!(reader.watchable());
        assert_eq!(
            plain(&reader.lay_out(60)).last().map(String::as_str),
            Some("Watch: https://www.youtube.com/watch?v=dQw4w9WgXcQ")
        );
        assert!(super::super::ui::footer_hints(&reader).contains(&("w", "watch")));
    }
    let reader = ReaderModal::open(&story(BodyStatus::Unavailable));
    assert!(!reader.watchable());
    assert!(!super::super::ui::footer_hints(&reader).contains(&("w", "watch")));
}

/// Scrolling stops at either end of what the last draw measured, and a
/// page keeps one line of the last.
#[test]
fn scroll_clamps_to_the_drawn_extent() {
    let reader = ReaderModal::open(&story(BodyStatus::Pending));
    let view = reader.view();
    view.scroll(5);
    assert_eq!(view.offset(), 0, "nothing drawn, nothing to scroll");

    view.set_viewport(ScrollViewport {
        popup: Rect::new(0, 0, 40, 20),
        body: Rect::new(2, 1, 30, 10),
        close: Rect::default(),
        track: Rect::new(38, 1, 1, 10),
        content_height: 25,
    });
    view.scroll(-3);
    assert_eq!(view.offset(), 0);
    reader.page(1);
    assert_eq!(view.offset(), 9, "a page keeps one line");
    view.scroll_to_bottom();
    assert_eq!(view.offset(), 15);
    view.scroll(4);
    assert_eq!(view.offset(), 15);
    view.scroll_to_top();
    assert_eq!(view.offset(), 0);
}
