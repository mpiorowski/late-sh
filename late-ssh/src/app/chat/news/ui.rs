//! The News room's list: the reader's shelf above the feed, two rows a
//! story, the selected one opened out with its link, its summary and its
//! art. Every row is laid out first (`lay_out`, pure) and the draw paints
//! the window of rows that keeps the selected story in view.

use crate::app::chat::list_ui::{draw_mine_only_status, filtered_list_areas};
use crate::app::chat::news::reader::state::domain_label;
use crate::app::chat::news::state::is_unread;
use crate::app::chat::news::svc::is_youtube_url;
use crate::app::chat::ui::truncate_cells;
use crate::app::chat::ui_text::format_news_ascii_art_for_display;
use crate::app::common::markdown::wrap_plain_line;
use crate::app::common::primitives::format_relative_time_short;
use crate::app::common::theme;
use crate::app::video::state::position_label;
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

use late_core::models::article::{Article, ArticleFeedItem, NEWS_SHARE_REWARD_CHIPS};
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
/// The bullet and its space: a point's later rows hang this far in.
const BULLET_HANG: usize = 2;

/// One row of the list as drawn: its text, and the slice of art that sits
/// at its right.
pub(crate) struct ListRow {
    pub(crate) line: Line<'static>,
    pub(crate) art: Option<String>,
}

pub(crate) struct ListLayout {
    pub(crate) rows: Vec<ListRow>,
    /// The rows of the selected story: highlighted, and the ones a draw
    /// must keep in view.
    pub(crate) selected: Range<usize>,
}

pub fn draw_article_list(frame: &mut Frame, area: Rect, view: &ArticleListView<'_>) {
    let (status_area, list_area) = filtered_list_areas(area, view.mine_only);
    if let Some(status_area) = status_area {
        draw_mine_only_status(frame, status_area, "news");
    }

    if view.articles.is_empty() {
        let text = Text::from(format!(
            "No news yet. Press 'i' to share a link and earn {NEWS_SHARE_REWARD_CHIPS} chips."
        ));
        let empty_p = Paragraph::new(text).style(Style::default().fg(theme::TEXT_DIM()));
        frame.render_widget(empty_p, list_area);
        return;
    }

    let ListLayout { rows, selected } = lay_out(view, list_area.width);
    let height = usize::from(list_area.height);
    let offset = scroll_offset(&selected, height);
    let art_x = list_area.right().saturating_sub(ART_WIDTH);
    for (row_index, row) in rows.into_iter().enumerate().skip(offset).take(height) {
        let y = list_area.y + (row_index - offset) as u16;
        let row_area = Rect::new(list_area.x, y, list_area.width, 1);
        let style = theme::row_style(selected.contains(&row_index));
        frame.render_widget(Paragraph::new(row.line).style(style), row_area);
        if let Some(art) = row.art {
            let art_area = Rect::new(art_x, y, ART_WIDTH, 1);
            frame.render_widget(
                Paragraph::new(art).style(style.fg(theme::AMBER_DIM())),
                art_area,
            );
        }
    }
}

/// The first row drawn: the selected story's rows stay in view, the list
/// scrolling just enough to show its last row, so moving down the list
/// walks it up the screen and moving back up walks it down.
pub(crate) fn scroll_offset(selected: &Range<usize>, height: usize) -> usize {
    selected.end.saturating_sub(height).min(selected.start)
}

/// Every row of the list for `width` columns: a rule over the shelf and
/// one over the feed when there is a shelf, two rows a story (its title,
/// then who shared it, when, where, and what reading it takes) with a
/// blank row between stories, and the selected story opened out with its
/// link and up to three bullets of its summary, its art beside them when
/// the list is wide enough.
pub(crate) fn lay_out(view: &ArticleListView<'_>, width: u16) -> ListLayout {
    let mut rows: Vec<ListRow> = Vec::new();
    let mut selected = 0..0;
    let selected_index = view
        .selected_index
        .min(view.articles.len().saturating_sub(1));
    let sectioned = view.saved_len > 0;
    let full_width = usize::from(width);

    for (index, item) in view.articles.iter().enumerate() {
        // A blank row between stories, and above the feed's rule.
        if index > 0 {
            rows.push(ListRow {
                line: Line::default(),
                art: None,
            });
        }
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
        let with_art = is_selected && width >= ART_MIN_LIST_WIDTH;
        let art_columns = if with_art {
            usize::from(ART_WIDTH) + 1
        } else {
            0
        };
        let text_width = full_width.saturating_sub(GUTTER + art_columns);
        let mut lines = story_rows(item, view, index < view.saved_len, text_width);
        if is_selected {
            lines.extend(detail_rows(&item.article, text_width));
        }
        let art = if with_art {
            ascii_preview_if_fit(&item.article.ascii_art, usize::from(ART_WIDTH), lines.len())
        } else {
            Vec::new()
        };
        let mut art = art.into_iter();
        let start = rows.len();
        for line in lines {
            rows.push(ListRow {
                line,
                art: art.next(),
            });
        }
        if is_selected {
            selected = start..rows.len();
        }
    }
    ListLayout { rows, selected }
}

/// What the selected story opens out with: its link, then up to
/// `SUMMARY_BULLETS` bullets of its summary.
fn detail_rows(article: &Article, text_width: usize) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(vec![
        Span::raw(" ".repeat(GUTTER)),
        Span::styled(
            truncate_cells(&article.url, text_width),
            Style::default()
                .fg(theme::TEXT_FAINT())
                .add_modifier(Modifier::ITALIC),
        ),
    ])];
    for point in summary_points(&article.summary)
        .iter()
        .take(SUMMARY_BULLETS)
    {
        for text in bullet_rows(point, text_width) {
            lines.push(Line::from(vec![
                Span::raw(" ".repeat(GUTTER)),
                Span::styled(text, Style::default().fg(theme::TEXT())),
            ]));
        }
    }
    lines
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
            truncate_cells(article.title.trim(), text_width),
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        ),
    ]);

    let separator = || Span::styled(" · ", Style::default().fg(theme::TEXT_FAINT()));
    let mut meta = vec![
        Span::raw(" ".repeat(GUTTER)),
        // Amber but not bold: the title leads the story.
        Span::styled(
            format!("@{}", item.author_username),
            Style::default().fg(theme::AMBER()),
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

/// `── label ───…` across the width: the sidebar's rule shape, in the News
/// list's own colours (a bold dim amber label on `BORDER`).
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
        art: None,
    }
}

/// A summary point behind its bullet, wrapped to `width` with its later
/// rows hung under the text, the way the reader sets bullets: at most
/// `BULLET_ROWS` rows, the last one clipped with an ellipsis when the point
/// runs on.
fn bullet_rows(point: &str, width: usize) -> Vec<String> {
    let text_width = width.saturating_sub(BULLET_HANG).max(1);
    let mut wrapped = wrap_plain_line(point, text_width);
    if wrapped.len() > BULLET_ROWS {
        wrapped.truncate(BULLET_ROWS);
        if let Some(last) = wrapped.last_mut() {
            *last = truncate_cells(&format!("{last} …"), text_width);
        }
    }
    let mut rows = Vec::new();
    for (index, text) in wrapped.into_iter().enumerate() {
        let lead = if index == 0 { "• " } else { "  " };
        rows.push(format!("{lead}{text}"));
    }
    rows
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

/// The summary's points, one a line, each without the bullet mark it came
/// with.
fn summary_points(text: &str) -> Vec<String> {
    text.replace("\\n", "\n")
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| {
            line.trim_start_matches('•')
                .trim_start_matches('-')
                .trim()
                .to_string()
        })
        .collect()
}

pub(crate) fn split_summary_bullets(text: &str) -> Vec<String> {
    summary_points(text)
        .into_iter()
        .map(|point| format!("• {point}"))
        .collect()
}

#[cfg(test)]
#[path = "ui_test.rs"]
mod ui_test;
