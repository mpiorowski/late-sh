use super::{
    ArticleListView, ascii_preview_if_fit, draw_article_list, lay_out, lounge_card_lines,
    scroll_offset,
};
use chrono::Utc;
use late_core::models::article::{Article, ArticleFeedItem};
use late_core::models::article_body::BodyStatus;
use late_core::models::article_read::WatchPosition;
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
            lounge_message_id: None,
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

/// The list as drawn on a `width` by `height` terminal, one string a row.
fn drawn(view: &ArticleListView<'_>, width: u16, height: u16) -> Vec<String> {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
    terminal
        .draw(|frame| draw_article_list(frame, frame.area(), view))
        .expect("draw");
    let buffer = terminal.backend().buffer();
    (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect()
}

fn view<'a>(
    articles: &'a [ArticleFeedItem],
    saved_len: usize,
    selected_index: usize,
    marker: &'a HashSet<Uuid>,
    watch_positions: &'a HashMap<Uuid, WatchPosition>,
) -> ArticleListView<'a> {
    ArticleListView {
        articles,
        saved_len,
        saved_count: saved_len,
        selected_index,
        marker,
        watch_positions,
        mine_only: false,
    }
}

/// The card's meta row, as drawn.
fn meta_row(item: ArticleFeedItem) -> String {
    let marker = HashSet::new();
    let positions = HashMap::new();
    let articles = [item];
    drawn(&view(&articles, 0, 0, &marker, &positions), 140, 12)
        .into_iter()
        .find(|row| row.contains("@mat ·"))
        .expect("a meta row")
}

/// The meta row ends in the reading time once the body is in, `link` when
/// there is nothing to read here, and the domain while the fetch is pending.
#[test]
fn card_meta_reads_the_body_status() {
    assert!(meta_row(card(BodyStatus::Ready { reading_minutes: 7 })).ends_with("· 7 min read"));
    assert!(meta_row(card(BodyStatus::Unavailable)).ends_with("· link"));
    assert!(meta_row(card(BodyStatus::Pending)).ends_with("· example.com"));
}

/// A video reads `watch` for everyone, whatever its body row says; any
/// other story with no text still reads `link`.
#[test]
fn card_meta_reads_watch_for_a_video() {
    let video = "https://www.youtube.com/watch?v=dQw4w9WgXcQ";
    assert!(meta_row(card_at(video, BodyStatus::Unavailable)).ends_with("youtube.com · watch"));
    assert!(meta_row(card_at(video, BodyStatus::Pending)).ends_with("youtube.com · watch"));
    assert!(
        meta_row(card_at("https://x.com/a/status/1", BodyStatus::Unavailable))
            .ends_with("x.com · link")
    );
}

/// With a shelf, a rule heads it and another heads the feed. Every story
/// shows whole, selected or not: title, meta, link and bullets. Without a
/// shelf a plain rule parts one story from the next.
#[test]
fn the_list_sections_the_shelf_over_the_feed_and_shows_every_story_whole() {
    let positions = HashMap::new();
    let mut kept = card_at("https://example.com/kept", BodyStatus::Unavailable);
    kept.article.title = "Kept for later".to_string();
    let articles = [kept, card(BodyStatus::Ready { reading_minutes: 7 })];
    // Both read, so no dot gets in the way of the rows.
    let marker: HashSet<Uuid> = articles.iter().map(|item| item.article.id).collect();

    let rows = drawn(&view(&articles, 1, 1, &marker, &positions), 80, 14);
    assert!(
        rows[0].starts_with("━━ saved · 1 of 10 ━━"),
        "{:?}",
        rows[0]
    );
    assert_eq!(rows[1], "  Kept for later");
    assert!(rows[2].starts_with("  @mat ·"));
    assert_eq!(rows[3], "  https://example.com/kept");
    assert_eq!(rows[4], "  • one");
    assert_eq!(rows[5], "  • two");
    assert_eq!(rows[6], "  • three");
    assert!(rows[7].starts_with("━━ latest ━━"), "{:?}", rows[7]);
    assert_eq!(rows[8], "  Why LISTEN/NOTIFY stops scaling");
    assert!(rows[9].ends_with("· 7 min read"));
    assert_eq!(rows[10], "  https://example.com/listen-notify");
    assert_eq!(rows[11], "  • one");
    assert_eq!(rows[12], "  • two");
    assert_eq!(rows[13], "  • three");

    let rows = drawn(&view(&articles, 0, 0, &marker, &positions), 80, 13);
    assert_eq!(rows[0], "  Kept for later");
    assert_eq!(rows[5], "  • three");
    assert_eq!(rows[6], "─".repeat(80));
    assert_eq!(rows[7], "  Why LISTEN/NOTIFY stops scaling");
    assert_eq!(rows[12], "  • three");
}

/// The unread dot marks a feed story the visit's marker has no read for,
/// and never a story on the shelf: the shelf is the reader's own list.
#[test]
fn the_unread_dot_marks_the_feed_and_not_the_shelf() {
    let marker = HashSet::new();
    let positions = HashMap::new();
    let articles = [card(BodyStatus::Unavailable), card(BodyStatus::Unavailable)];
    let rows = drawn(&view(&articles, 1, 1, &marker, &positions), 80, 14);
    assert!(rows[1].starts_with("  Why"), "{:?}", rows[1]);
    assert!(rows[8].starts_with("● Why"), "{:?}", rows[8]);
}

/// A bullet that runs on takes two rows at most, the second hung under the
/// text after the bullet and cut with an ellipsis, so a story stays a card
/// and not a page.
#[test]
fn a_long_bullet_is_cut_at_two_rows() {
    let marker = HashSet::new();
    let positions = HashMap::new();
    let mut item = card(BodyStatus::Unavailable);
    item.article.summary = format!("• {}", "word ".repeat(60));
    let articles = [item];
    let layout = lay_out(&view(&articles, 0, 0, &marker, &positions), 60);
    // Title, meta, link, then the bullet's two rows.
    assert_eq!(layout.rows.len(), 5);
    assert_eq!(layout.selected, 0..5);
    let text = |row: usize| -> String {
        layout.rows[row]
            .line
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    };
    assert!(text(3).starts_with("  • word"), "{:?}", text(3));
    assert!(text(4).starts_with("    word"), "{:?}", text(4));
    assert!(text(4).trim_end().ends_with('…'), "{:?}", text(4));
}

/// The draw scrolls just enough to keep the selected story's last row in
/// view, and no further than its first row on a short screen.
#[test]
fn the_scroll_keeps_the_selected_story_in_view() {
    assert_eq!(scroll_offset(&(0..2), 10), 0);
    assert_eq!(scroll_offset(&(8..14), 10), 4);
    assert_eq!(scroll_offset(&(8..14), 3), 8);
    assert_eq!(scroll_offset(&(8..14), 0), 8);
}

/// A #lounge card is the story in six rows, the art down the right: the
/// title, a row per bullet, where it is from and what it takes, and the line
/// that sends the room to News. Too narrow for the art, every row keeps the
/// whole width, and a row cut short ends in an ellipsis even where the
/// width runs out exactly at the end of one of its parts (the meta row at 24
/// would otherwise drop "Enter to open" without a mark).
#[test]
fn a_lounge_card_is_the_story_beside_its_art() {
    let mut item = card(BodyStatus::Ready { reading_minutes: 7 });
    item.article.ascii_art = "aaaa\nbbbb\ncccc\ndddd\neeee\nffff".to_string();
    let text = |line: &ratatui::text::Line<'_>| -> String {
        line.spans.iter().map(|span| span.content.as_ref()).collect()
    };

    let rows: Vec<String> = lounge_card_lines(&item, 60).iter().map(text).collect();
    let beside = |words: &str, art: &str| {
        let pad = 47 - unicode_width::UnicodeWidthStr::width(words);
        format!("{words}{} {art}", " ".repeat(pad))
    };
    assert_eq!(
        rows,
        vec![
            beside("📰 Why LISTEN/NOTIFY stops scaling", "aaaa"),
            beside("• one", "bbbb"),
            beside("• two", "cccc"),
            beside("• three", "dddd"),
            beside("example.com · 7 min read · Enter to open", "eeee"),
            beside("share a link in News · +500 chips", "ffff"),
        ]
    );

    let rows: Vec<String> = lounge_card_lines(&item, 24).iter().map(text).collect();
    assert_eq!(
        rows,
        vec![
            "📰 Why LISTEN/NOTIFY st…".to_string(),
            "• one".to_string(),
            "• two".to_string(),
            "• three".to_string(),
            "example.com · 7 min rea…".to_string(),
            "share a link in News · …".to_string(),
        ]
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
