//! The News room's list: the reader's shelf above the feed, every story
//! with its link, its summary and its art, a rule between stories. Every
//! row is laid out first (`lay_out`, pure) and the draw paints the window
//! of rows that keeps the selected story in view.

use crate::app::chat::list_ui::{draw_mine_only_status, filtered_list_areas};
use crate::app::chat::news::reader::state::domain_label;
use crate::app::chat::news::state::is_unread;
use crate::app::chat::ui::truncate_cells;
use crate::app::chat::ui_text::format_news_ascii_art_for_display;
use crate::app::common::markdown::wrap_plain_line;
use crate::app::common::primitives::format_relative_time_short;
use crate::app::common::theme;
use crate::app::video::state::{is_watchable, position_label};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::Paragraph,
};
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};
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
/// The art beside each story, the width the service draws it at.
const ART_WIDTH: u16 = 12;
/// Narrower than this a story keeps its whole width for text.
const ART_MIN_LIST_WIDTH: u16 = 90;
/// The summary's bullets on a story, and the rows each may take.
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
/// one over the feed when there is a shelf, and every story whole (its
/// title, then who shared it, when, where, and what reading it takes, then
/// its link and up to three bullets of its summary, its art beside them
/// when the list is wide enough) with a plain rule between stories.
pub(crate) fn lay_out(view: &ArticleListView<'_>, width: u16) -> ListLayout {
    let mut rows: Vec<ListRow> = Vec::new();
    let mut selected = 0..0;
    let selected_index = view
        .selected_index
        .min(view.articles.len().saturating_sub(1));
    let sectioned = view.saved_len > 0;
    let full_width = usize::from(width);
    let with_art = width >= ART_MIN_LIST_WIDTH;
    let art_columns = if with_art {
        usize::from(ART_WIDTH) + 1
    } else {
        0
    };
    let text_width = full_width.saturating_sub(GUTTER + art_columns);

    for (index, item) in view.articles.iter().enumerate() {
        // A section's rule heads its first story; any other story is
        // parted from the one above by a plain rule.
        if sectioned && index == 0 {
            rows.push(section_rule(
                &format!("saved · {} of {NEWS_SAVED_LIMIT}", view.saved_count),
                full_width,
            ));
        } else if sectioned && index == view.saved_len {
            rows.push(section_rule("latest", full_width));
        } else if index > 0 {
            rows.push(story_rule(full_width));
        }
        let mut lines = story_rows(item, view, index < view.saved_len, text_width);
        lines.extend(detail_rows(&item.article, text_width));
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
        if index == selected_index {
            selected = start..rows.len();
        }
    }
    ListLayout { rows, selected }
}

/// What follows a story's title and meta: its link, then up to
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

/// `━━ label ━━━…` across the width: the sidebar's rule shape, drawn heavy
/// so a section stands out from the light rules between stories, in the
/// News list's own colours (a bold dim amber label on `BORDER`).
fn section_rule(label: &str, width: usize) -> ListRow {
    let lead = "━━ ";
    let used = UnicodeWidthStr::width(lead) + UnicodeWidthStr::width(label) + 1;
    let tail = "━".repeat(width.saturating_sub(used));
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

/// The `───…` that parts one story from the next, in `BORDER`.
fn story_rule(width: usize) -> ListRow {
    ListRow {
        line: Line::from(Span::styled(
            "─".repeat(width),
            Style::default().fg(theme::BORDER()),
        )),
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

/// The #lounge card is this wide at least before its art goes beside it.
const LOUNGE_ART_MIN_WIDTH: usize = 40;

/// A News share's card in #lounge, drawn in place of its message's line
/// (`📰 Title url`), in `width` columns: the title, up to three bullets of
/// the summary one row each, where it is from and what it takes, and the
/// line that sends the room to News; the story's art at the right when the
/// card is wide enough. Chat lays the rows under the author's header.
pub(crate) fn lounge_card_lines(item: &ArticleFeedItem, width: usize) -> Vec<Line<'static>> {
    let article = &item.article;
    let with_art = width >= LOUNGE_ART_MIN_WIDTH;
    let text_width = match with_art {
        true => width.saturating_sub(usize::from(ART_WIDTH) + 1),
        false => width,
    };
    let faint = Style::default().fg(theme::TEXT_FAINT());

    let mut rows: Vec<Vec<Span<'static>>> = vec![vec![Span::styled(
        truncate_cells(&format!("📰 {}", article.title.trim()), text_width),
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD),
    )]];
    for point in summary_points(&article.summary)
        .iter()
        .take(SUMMARY_BULLETS)
    {
        rows.push(vec![Span::styled(
            truncate_cells(&format!("• {point}"), text_width),
            Style::default().fg(theme::CHAT_BODY()),
        )]);
    }

    let mut meta = vec![Span::styled(domain_label(&article.url), faint)];
    if let Some(label) = reading_label(item, &HashMap::new()) {
        meta.push(Span::styled(" · ", faint));
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
    meta.push(Span::styled(" · Enter to open", faint));
    rows.push(meta);
    rows.push(vec![
        Span::styled("share a link in News · ", faint),
        Span::styled(
            format!("+{NEWS_SHARE_REWARD_CHIPS} chips"),
            Style::default().fg(theme::AMBER_DIM()),
        ),
    ]);

    let art = match with_art {
        true => ascii_preview_if_fit(&article.ascii_art, usize::from(ART_WIDTH), rows.len()),
        false => Vec::new(),
    };
    let mut art = art.into_iter();
    rows.into_iter()
        .map(|spans| {
            let spans = clip_spans(spans, text_width);
            match art.next() {
                Some(piece) => {
                    let used: usize = spans.iter().map(Span::width).sum();
                    let mut spans = spans;
                    spans.push(Span::raw(" ".repeat(text_width - used + 1)));
                    spans.push(Span::styled(piece, Style::default().fg(theme::AMBER_DIM())));
                    Line::from(spans)
                }
                None => Line::from(spans),
            }
        })
        .collect()
}

/// `spans` cut to `width` columns: a card's meta row on a narrow chat. A
/// row that runs over keeps `width - 1` columns and ends with an ellipsis in
/// the style of the span it cuts into, even when the cut falls between two
/// spans, so a cut row always reads as cut.
fn clip_spans(spans: Vec<Span<'static>>, width: usize) -> Vec<Span<'static>> {
    let total: usize = spans.iter().map(Span::width).sum();
    if total <= width {
        return spans;
    }
    let mut room = width.saturating_sub(1);
    let mut clipped = Vec::new();
    for span in spans {
        let span_width = span.width();
        if span_width <= room {
            room -= span_width;
            clipped.push(span);
            continue;
        }
        let mut kept = String::new();
        for ch in span.content.chars() {
            let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0);
            if ch_width > room {
                break;
            }
            room -= ch_width;
            kept.push(ch);
        }
        kept.push('…');
        clipped.push(Span::styled(kept, span.style));
        break;
    }
    clipped
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
    if is_watchable(&item.article.url) {
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
