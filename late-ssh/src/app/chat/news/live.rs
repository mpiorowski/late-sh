//! A News article on the live strip (`app/live/`): somebody shared a link,
//! so the room sees what it is and who brought it, and can open it with a
//! key. The article's ASCII art sits centred in the picture column; the
//! words beside it are the title, the first lines of the summary, and who
//! shared it.

use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};
use uuid::Uuid;

use late_core::models::article::ArticleFeedItem;

use crate::app::{
    chat::ui_text::{format_news_ascii_art_for_display, split_summary_bullets},
    common::theme,
    live::{
        pick::{LiveCandidate, LiveSource},
        ui::{PICTURE_COLS, PICTURE_ROWS, StripBody, truncate_chars},
    },
};

/// Summary lines the words carry, under the title.
const SUMMARY_ROWS: usize = 2;

/// A News article as the live strip paints it.
#[derive(Clone)]
pub struct ArticleStripView {
    pub item: ArticleFeedItem,
}

/// Every article in the News snapshot, stamped with when it was shared: a
/// link is news when somebody brings it.
pub(crate) fn candidates(articles: &[ArticleFeedItem]) -> Vec<LiveCandidate> {
    articles
        .iter()
        .map(|item| LiveCandidate {
            source: LiveSource::NewsArticle(item.article.id),
            updated: item.article.created,
            aimed_at: None,
        })
        .collect()
}

/// One article as the strip paints it. `None` once it left the snapshot
/// (deleted, or pushed out by newer shares).
pub(crate) fn view(articles: &[ArticleFeedItem], article_id: Uuid) -> Option<ArticleStripView> {
    articles
        .iter()
        .find(|item| item.article.id == article_id)
        .map(|item| ArticleStripView { item: item.clone() })
}

pub(crate) fn body(budget: usize, article: &ArticleStripView) -> StripBody {
    StripBody {
        picture: picture_lines(article),
        words: word_rows(budget, article),
        // Nothing about a shared link is happening right now: the strip
        // being up is the whole of the news.
        glow: false,
    }
}

/// `news mat · Article Title`, after the rule label.
pub(crate) fn compact_spans(rest: u16, article: &ArticleStripView) -> Vec<Span<'static>> {
    let rest = usize::from(rest);
    let lead = format!("news {} · ", sharer(article));
    let lead = truncate_chars(&lead, rest);
    let left = rest.saturating_sub(lead.chars().count());
    vec![
        Span::styled(lead, Style::default().fg(theme::TEXT_DIM())),
        Span::styled(
            truncate_chars(&article.item.article.title, left),
            Style::default().fg(theme::TEXT()),
        ),
    ]
}

/// The article's ASCII art, centred in the picture column.
fn picture_lines(article: &ArticleStripView) -> Vec<Line<'static>> {
    let columns = usize::from(PICTURE_COLS);
    let rows =
        format_news_ascii_art_for_display(&article.item.article.ascii_art, PICTURE_ROWS as usize);
    let widest = rows
        .iter()
        .map(|row| row.chars().count())
        .max()
        .unwrap_or(0)
        .min(columns);
    let lead = " ".repeat((columns - widest) / 2);
    rows.into_iter()
        .map(|row| {
            Line::from(vec![
                Span::raw(lead.clone()),
                Span::styled(
                    truncate_chars(&row, widest),
                    Style::default().fg(theme::AMBER_DIM()),
                ),
            ])
        })
        .collect()
}

/// The words beside the picture, one entry per picture row: the title, the
/// first lines of the summary, who shared it, then the keys that read it
/// and reply to it.
fn word_rows(budget: usize, article: &ArticleStripView) -> Vec<Vec<Span<'static>>> {
    let mut rows: Vec<Vec<Span<'static>>> = (0..PICTURE_ROWS).map(|_| Vec::new()).collect();
    if budget == 0 {
        return rows;
    }
    rows[1] = vec![Span::styled(
        truncate_chars(&article.item.article.title, budget),
        Style::default()
            .fg(theme::TEXT())
            .add_modifier(Modifier::BOLD),
    )];
    for (row, bullet) in split_summary_bullets(&article.item.article.summary)
        .into_iter()
        .take(SUMMARY_ROWS)
        .enumerate()
    {
        rows[2 + row] = vec![Span::styled(
            truncate_chars(&bullet, budget),
            Style::default().fg(theme::TEXT_DIM()),
        )];
    }
    rows[4] = event_spans(budget, article);
    rows[6] = vec![Span::styled(
        truncate_chars("o read · r reply", budget),
        Style::default().fg(theme::TEXT_FAINT()),
    )];
    rows
}

fn sharer(article: &ArticleStripView) -> String {
    if article.item.author_username.is_empty() {
        "somebody".to_string()
    } else {
        article.item.author_username.clone()
    }
}

/// `mat shared it`, the sharer in amber.
fn event_spans(budget: usize, article: &ArticleStripView) -> Vec<Span<'static>> {
    let name = truncate_chars(&sharer(article), budget);
    let left = budget.saturating_sub(name.chars().count());
    vec![
        Span::styled(
            name,
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            truncate_chars(" shared it", left),
            Style::default().fg(theme::TEXT()),
        ),
    ]
}

#[cfg(test)]
#[path = "live_test.rs"]
mod live_test;
