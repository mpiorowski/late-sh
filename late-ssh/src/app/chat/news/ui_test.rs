use super::{ArticleListView, ascii_preview_if_fit, draw_article_list};
use chrono::Utc;
use late_core::models::article::{Article, ArticleFeedItem};
use late_core::models::article_body::BodyStatus;
use ratatui::{Terminal, backend::TestBackend};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

fn card(body: BodyStatus) -> ArticleFeedItem {
    card_at("https://example.com/listen-notify", body)
}

fn card_at(url: &str, body: BodyStatus) -> ArticleFeedItem {
    let now = Utc::now();
    ArticleFeedItem {
        article: Article {
            id: Uuid::now_v7(),
            created: now,
            updated: now,
            user_id: Uuid::now_v7(),
            url: url.to_string(),
            title: "Why LISTEN/NOTIFY stops scaling".to_string(),
            summary: "• one\n• two\n• three".to_string(),
            ascii_art: String::new(),
        },
        author_username: "mat".to_string(),
        body,
    }
}

/// The card's meta row, as drawn.
fn meta_row(item: ArticleFeedItem) -> String {
    let mut terminal = Terminal::new(TestBackend::new(140, 12)).expect("terminal");
    let marker = HashSet::new();
    let articles = [item];
    terminal
        .draw(|frame| {
            draw_article_list(
                frame,
                frame.area(),
                &ArticleListView {
                    articles: &articles,
                    selected_index: 0,
                    marker: &marker,
                    watch_positions: &HashMap::new(),
                    mine_only: false,
                },
            )
        })
        .expect("draw");
    let buffer = terminal.backend().buffer();
    (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .find(|row| row.contains("@mat - "))
        .expect("a meta row")
        .trim_end()
        .to_string()
}

/// The meta row ends in the reading time once the body is in, `link` when
/// there is nothing to read here, and nothing while the fetch is pending.
#[test]
fn card_meta_reads_the_body_status() {
    assert!(meta_row(card(BodyStatus::Ready { reading_minutes: 7 })).ends_with("UTC - 7 min read"));
    assert!(meta_row(card(BodyStatus::Unavailable)).ends_with("UTC - link"));
    assert!(meta_row(card(BodyStatus::Pending)).ends_with("UTC"));
}

/// A video reads `watch` for everyone, whatever its body row says; any
/// other story with no text still reads `link`.
#[test]
fn card_meta_reads_watch_for_a_video() {
    let video = "https://www.youtube.com/watch?v=dQw4w9WgXcQ";
    assert!(meta_row(card_at(video, BodyStatus::Unavailable)).ends_with("UTC - watch"));
    assert!(meta_row(card_at(video, BodyStatus::Pending)).ends_with("UTC - watch"));
    assert!(
        meta_row(card_at("https://x.com/a/status/1", BodyStatus::Unavailable))
            .ends_with("UTC - link")
    );
}

#[test]
fn ascii_preview_keeps_original_lines_when_fit() {
    let input = "abcd\nefgh\nijkl\nmnop";
    let out = ascii_preview_if_fit(input, 4, 2);
    assert_eq!(out, vec!["abcd".to_string(), "efgh".to_string()]);
}

#[test]
fn ascii_preview_hides_art_when_width_too_small() {
    let out = ascii_preview_if_fit("abcdef\n123456", 5, 6);
    assert!(out.is_empty());
}

#[test]
fn ascii_preview_returns_empty_for_empty_input() {
    assert!(ascii_preview_if_fit("", 10, 10).is_empty());
}

#[test]
fn ascii_preview_returns_empty_for_zero_dimensions() {
    assert!(ascii_preview_if_fit("abc", 0, 5).is_empty());
    assert!(ascii_preview_if_fit("abc", 5, 0).is_empty());
}

#[test]
fn ascii_preview_drops_blank_lines_before_width_check() {
    let out = ascii_preview_if_fit("\n   \n ab \n\\n cd  \n", 3, 6);
    assert_eq!(out, vec![" ab".to_string(), " cd".to_string()]);
}
