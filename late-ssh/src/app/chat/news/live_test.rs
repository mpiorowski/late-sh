use chrono::{DateTime, TimeZone, Utc};
use late_core::models::article::Article;

use super::*;
use crate::app::{
    live::{
        state::LiveStripView,
        ui::{live_strip_compact_line, live_strip_lines},
    },
    lobby::daily::live_board::canvas_background,
};

const WIDTH: u16 = 80;

fn line_text(line: &Line<'_>) -> String {
    line.spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect::<String>()
        .trim_end()
        .to_string()
}

fn shared_at(secs: i64) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 9, 29, 21, 0, 0).unwrap() + chrono::Duration::seconds(secs)
}

fn article(n: u128, title: &str, shared_secs: i64) -> ArticleFeedItem {
    ArticleFeedItem {
        article: Article {
            id: Uuid::from_u128(n),
            created: shared_at(shared_secs),
            updated: shared_at(shared_secs),
            user_id: Uuid::from_u128(99),
            url: format!("https://example.com/{n}"),
            title: title.to_string(),
            summary: "• terminals are back\n• ssh is the new browser\n• a third point".to_string(),
            ascii_art: "############\n#  late.sh #\n############".to_string(),
        },
        author_username: "mat".to_string(),
    }
}

#[test]
fn every_article_in_the_snapshot_is_news_from_when_it_was_shared() {
    let articles = vec![article(2, "Newer", 60), article(1, "Older", 0)];
    let stamps: Vec<(LiveSource, DateTime<Utc>)> = candidates(&articles)
        .iter()
        .map(|candidate| (candidate.source, candidate.updated))
        .collect();
    assert_eq!(
        stamps,
        vec![
            (LiveSource::NewsArticle(Uuid::from_u128(2)), shared_at(60)),
            (LiveSource::NewsArticle(Uuid::from_u128(1)), shared_at(0)),
        ]
    );
}

#[test]
fn the_strip_shows_the_art_centred_beside_what_it_is_and_who_shared_it() {
    let articles = vec![article(1, "The terminal renaissance", 0)];
    let strip = LiveStripView::Article(view(&articles, Uuid::from_u128(1)).unwrap());
    let lines: Vec<String> = live_strip_lines(WIDTH, &strip, canvas_background())
        .iter()
        .map(line_text)
        .collect();

    let words = " ".repeat(23);
    let art = |row: &str| format!("    {row}       ");
    assert_eq!(
        lines,
        vec![
            String::new(),
            format!("{words}The terminal renaissance"),
            format!("{}• terminals are back", art("############")),
            format!("{}• ssh is the new browser", art("#  late.sh #")),
            format!("{}mat shared it", art("############")),
            String::new(),
            format!("{words}o read · r reply"),
            String::new(),
            format!("── now {}", "─".repeat(73)),
        ]
    );
}

#[test]
fn the_one_row_form_says_who_shared_what() {
    let articles = vec![article(1, "The terminal renaissance", 0)];
    let strip = LiveStripView::Article(view(&articles, Uuid::from_u128(1)).unwrap());
    assert_eq!(
        line_text(&live_strip_compact_line(WIDTH, &strip)),
        "── now news mat · The terminal renaissance"
    );
}

#[test]
fn an_article_that_left_the_snapshot_has_no_view() {
    let articles = vec![article(1, "Kept", 0)];
    assert!(view(&articles, Uuid::from_u128(2)).is_none());
}
