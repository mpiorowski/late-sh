//! The News room's list: the reader's shelf above the feed, two rows a
//! story, the selected one opened out with its link, its summary and its
//! art. Every row is laid out first (`lay_out`, pure) and the draw paints
//! the window of rows that keeps the selected story in view.

use crate::app::chat::list_ui::{draw_mine_only_status, filtered_list_areas};
use crate::app::chat::news::reader::state::domain_label;
use crate::app::chat::news::state::is_unread;
use crate::app::chat::news::svc::is_youtube_url;
use crate::app::chat::news::watch::state::position_label;
use crate::app::chat::ui_text::format_news_ascii_art_for_display;
use crate::app::common::markdown::wrap_plain_line;
use crate::app::common::primitives::format_relative_time_short;
use crate::app::common::theme;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::Paragraph,
};
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use unicode_width::UnicodeWidthStr;
use uuid::Uuid;

use late_core::models::article::{ArticleFeedItem, NEWS_SHARE_REWARD_CHIPS};
use late_core::models::article_body::BodyStatus;
use late_core::models::article_read::WatchPosition;
use late_core::models::article_save::NEWS_SAVED_LIMIT;

pub struct ArticleListView<'a> {
    /// The shelf first, then the feed (`news::state::display_order`).
    pub articles: &'a [ArticleFeedItem],
    /// How many leading `articles` are the shelf.
    pub saved_len: usize,
    /// How many stories the shelf holds, filter or no filter.
    pub saved_count: usize,
    pub selected_index: usize,
    pub marker: &'a HashSet<Uuid>,
    /// Where the reader left each video they are partway through.
    pub watch_positions: &'a HashMap<Uuid, WatchPosition>,
    pub mine_only: bool,
}

/// The unread dot's column and the space after it.
const GUTTER: usize = 2;
/// The art beside the selected story, the width the service draws it at.
const ART_WIDTH: u16 = 12;
/// Narrower than this the selected story keeps its whole width for text.
const ART_MIN_LIST_WIDTH: u16 = 90;
/// The summary's bullets on the selected story, and the rows each may take.
const SUMMARY_BULLETS: usize = 3;
const BULLET_ROWS: usize = 2;

/// One row of the list as drawn: its text, whether it is part of the
/// selected story, and the slice of art that sits at its right.
pub(crate) struct ListRow {
    pub(crate) line: Line<'static>,
    pub(crate) selected: bool,
    pub(crate) art: Option<String>,
}

pub(crate) struct ListLayout {
    pub(crate) rows: Vec<ListRow>,
    /// The rows of the selected story, the ones a draw must keep in view.
    pub(crate) selected: Range<usize>,
}

pub fn draw_article_list(frame: &mut Frame, area: Rect, view: &ArticleListView<'_>) {
    let (status_area, list_area) = filtered_list_areas(area, view.mine_only);
    if let Some(status_area) = status_area {
        draw_mine_only_status(frame, status_area, "news");
    }
    if list_area.width == 0 || list_area.height == 0 {
        return;
    }

    if view.articles.is_empty() {
        let text = Text::from(format!(
            "No news yet. Press 'i' to share a link and earn {NEWS_SHARE_REWARD_CHIPS} chips."
        ));
        let empty_p = Paragraph::new(text).style(Style::default().fg(theme::TEXT_DIM()));
        frame.render_widget(empty_p, list_area);
        return;
    }

    let layout = lay_out(view, list_area.width);
    let height = usize::from(list_area.height);
    let offset = scroll_offset(&layout.selected, height);
    let art_x = list_area.right().saturating_sub(ART_WIDTH);
    for (row_index, row) in layout.rows.iter().enumerate().skip(offset).take(height) {
        let y = list_area.y + (row_index - offset) as u16;
        let row_area = Rect::new(list_area.x, y, list_area.width, 1);
        let style = theme::row_style(row.selected);
        frame.render_widget(Paragraph::new(row.line.clone()).style(style), row_area);
        if let Some(art) = &row.art {
            let art_area = Rect::new(art_x, y, ART_WIDTH, 1);
            frame.render_widget(
                Paragraph::new(art.as_str()).style(style.fg(theme::AMBER_DIM())),
                art_area,
            );
        }
    }
}

/// The first row drawn: the selected story's rows stay in view, the list
/// scrolling just enough to show its last row, so moving down the list
/// walks it up the screen and moving back up walks it down.
pub(crate) fn scroll_offset(selected: &Range<usize>, height: usize) -> usize {
    if height == 0 {
        return selected.start;
    }
    selected.end.saturating_sub(height).min(selected.start)
}

/// Every row of the list for `width` columns: a rule over the shelf and
/// one over the feed when there is a shelf, two rows a story (its title,
/// then who shared it, when, where, and what reading it takes), and the
/// selected story opened out with its link and up to three bullets of its
/// summary, its art beside them when the list is wide enough.
pub(crate) fn lay_out(view: &ArticleListView<'_>, width: u16) -> ListLayout {
    let mut rows: Vec<ListRow> = Vec::new();
    let mut selected = 0..0;
    let selected_index = view
        .selected_index
        .min(view.articles.len().saturating_sub(1));
    let sectioned = view.saved_len > 0;
    let full_width = usize::from(width);

    for (index, item) in view.articles.iter().enumerate() {
        if sectioned && index == 0 {
            rows.push(section_rule(
                &format!("saved · {} of {NEWS_SAVED_LIMIT}", view.saved_count),
                full_width,
            ));
        }
        if sectioned && index == view.saved_len {
            rows.push(section_rule("latest", full_width));
        }
        let is_selected = index == selected_index;
        let on_shelf = index < view.saved_len;
        let start = rows.len();
        if is_selected {
            let with_art = width >= ART_MIN_LIST_WIDTH;
            let text_width = full_width.saturating_sub(
                GUTTER
                    + if with_art {
                        usize::from(ART_WIDTH) + 1
                    } else {
                        0
                    },
            );
            let mut lines = story_rows(item, view, on_shelf, text_width);
            lines.push(Line::from(vec![
                Span::raw(" ".repeat(GUTTER)),
                Span::styled(
                    clip_to_width(&item.article.url, text_width),
                    Style::default()
                        .fg(theme::TEXT_FAINT())
                        .add_modifier(Modifier::ITALIC),
                ),
            ]));
            for bullet in split_summary_bullets(&item.article.summary)
                .iter()
                .take(SUMMARY_BULLETS)
            {
                for text in bullet_rows(bullet, text_width) {
                    lines.push(Line::from(vec![
                        Span::raw(" ".repeat(GUTTER)),
                        Span::styled(text, Style::default().fg(theme::TEXT())),
                    ]));
                }
            }
            let art = if with_art {
                ascii_preview_if_fit(&item.article.ascii_art, usize::from(ART_WIDTH), lines.len())
            } else {
                Vec::new()
            };
            let mut art = art.into_iter();
            for line in lines {
                rows.push(ListRow {
                    line,
                    selected: true,
                    art: art.next(),
                });
            }
            selected = start..rows.len();
        } else {
            let text_width = full_width.saturating_sub(GUTTER);
            for line in story_rows(item, view, on_shelf, text_width) {
                rows.push(ListRow {
                    line,
                    selected: false,
                    art: None,
                });
            }
        }
    }
    ListLayout { rows, selected }
}

/// A story's two rows: the title behind its unread dot, and the meta.
fn story_rows(
    item: &ArticleFeedItem,
    view: &ArticleListView<'_>,
    on_shelf: bool,
    text_width: usize,
) -> Vec<Line<'static>> {
    let article = &item.article;
    // The dot says "new since your last visit", which is the feed's story
    // to tell: the shelf is a list the reader made.
    let unread = !on_shelf && is_unread(item, view.marker);
    let dot = if unread {
        Span::styled(
            "● ",
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::raw(" ".repeat(GUTTER))
    };
    let title = Line::from(vec![
        dot,
        Span::styled(
            clip_to_width(article.title.trim(), text_width),
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        ),
    ]);

    let separator = || Span::styled(" · ", Style::default().fg(theme::TEXT_FAINT()));
    let mut meta = vec![
        Span::raw(" ".repeat(GUTTER)),
        Span::styled(
            format!("@{}", item.author_username),
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        ),
        separator(),
        Span::styled(
            format_relative_time_short(article.created),
            Style::default().fg(theme::TEXT_DIM()),
        ),
        separator(),
        Span::styled(
            domain_label(&article.url),
            Style::default().fg(theme::TEXT_FAINT()),
        ),
    ];
    if let Some(label) = reading_label(item, view.watch_positions) {
        meta.push(separator());
        meta.push(Span::styled(
            label.text(),
            match label {
                ReadingLabel::Watch | ReadingLabel::Resume(_) => {
                    Style::default().fg(theme::AMBER())
                }
                ReadingLabel::Minutes(_) | ReadingLabel::Link => {
                    Style::default().fg(theme::TEXT_DIM())
                }
            },
        ));
    }
    vec![title, Line::from(meta)]
}

/// `── label ───…` across the width, the way the sidebar heads its parts.
fn section_rule(label: &str, width: usize) -> ListRow {
    let lead = "── ";
    let used = UnicodeWidthStr::width(lead) + UnicodeWidthStr::width(label) + 1;
    let tail = "─".repeat(width.saturating_sub(used));
    ListRow {
        line: Line::from(vec![
            Span::styled(lead, Style::default().fg(theme::BORDER())),
            Span::styled(
                label.to_string(),
                Style::default()
                    .fg(theme::AMBER_DIM())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!(" {tail}"), Style::default().fg(theme::BORDER())),
        ]),
        selected: false,
        art: None,
    }
}

/// A bullet wrapped to `width`, at most `BULLET_ROWS` rows, the last one
/// clipped with an ellipsis when the bullet runs on.
fn bullet_rows(bullet: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let wrapped = wrap_plain_line(bullet, width);
    if wrapped.len() <= BULLET_ROWS {
        return wrapped;
    }
    let mut rows: Vec<String> = wrapped.into_iter().take(BULLET_ROWS).collect();
    if let Some(last) = rows.last_mut() {
        *last = clip_to_width(&format!("{last} …"), width);
    }
    rows
}

/// `text` cut to `width` cells, an ellipsis marking the cut.
fn clip_to_width(text: &str, width: usize) -> String {
    if UnicodeWidthStr::width(text) <= width {
        return text.to_string();
    }
    let budget = width.saturating_sub(1);
    let mut out = String::new();
    let mut used = 0;
    for ch in text.chars() {
        let w = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + w > budget {
            break;
        }
        used += w;
        out.push(ch);
    }
    if width > 0 {
        out.push('…');
    }
    out
}

/// The card meta's last segment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReadingLabel {
    /// How long the story's text takes to read.
    Minutes(u16),
    /// No text to read here: an X post, a page the fetch could not read.
    Link,
    /// A video, for everyone: watching is the user's intent, and `w` says
    /// what their setup needs when it cannot play.
    Watch,
    /// A video this reader is partway through: `w` picks up here.
    Resume(WatchPosition),
}

impl ReadingLabel {
    pub(crate) fn text(self) -> String {
        match self {
            Self::Minutes(minutes) => format!("{minutes} min read"),
            Self::Link => "link".to_string(),
            Self::Watch => "watch".to_string(),
            Self::Resume(position) => format!("▶ {}", position_label(position)),
        }
    }
}

/// The card's reading label; nothing while a page's fetch is pending. A
/// video never gets text, so it reads `watch` whatever its body says, or
/// where this reader left it when they are partway through.
pub(crate) fn reading_label(
    item: &ArticleFeedItem,
    watch_positions: &HashMap<Uuid, WatchPosition>,
) -> Option<ReadingLabel> {
    if is_youtube_url(&item.article.url) {
        return Some(match watch_positions.get(&item.article.id) {
            Some(position) => ReadingLabel::Resume(*position),
            None => ReadingLabel::Watch,
        });
    }
    match item.body {
        BodyStatus::Ready { reading_minutes } => Some(ReadingLabel::Minutes(reading_minutes)),
        BodyStatus::Pending => None,
        BodyStatus::Unavailable => Some(ReadingLabel::Link),
    }
}

fn ascii_preview_if_fit(ascii_art: &str, target_width: usize, max_lines: usize) -> Vec<String> {
    if target_width == 0 || max_lines == 0 {
        return Vec::new();
    }

    let lines = format_news_ascii_art_for_display(ascii_art, max_lines);
    if lines.is_empty() {
        return Vec::new();
    }

    let max_line_width = lines
        .iter()
        .map(|line| UnicodeWidthStr::width(line.as_str()))
        .max()
        .unwrap_or(0);
    if max_line_width > target_width {
        return Vec::new();
    }

    lines.into_iter().take(max_lines).collect()
}

pub(crate) fn split_summary_bullets(text: &str) -> Vec<String> {
    text.replace("\\n", "\n")
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| {
            let stripped = line.trim_start_matches('•').trim_start_matches('-').trim();
            format!("• {stripped}")
        })
        .collect()
}

#[cfg(test)]
#[path = "ui_test.rs"]
mod ui_test;
