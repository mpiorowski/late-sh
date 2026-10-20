use crate::app::chat::list_ui::{draw_mine_only_status, filtered_list_areas};
use crate::app::chat::news::state::is_unread;
use crate::app::chat::ui_text::format_news_ascii_art_for_display;
use crate::app::common::primitives::format_relative_time;
use crate::app::common::theme;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
};
use std::collections::HashSet;
use unicode_width::UnicodeWidthStr;
use uuid::Uuid;

use late_core::models::article::{ArticleFeedItem, NEWS_SHARE_REWARD_CHIPS};
use late_core::models::article_body::BodyStatus;

pub struct ArticleListView<'a> {
    pub articles: &'a [ArticleFeedItem],
    pub selected_index: usize,
    pub marker: &'a HashSet<Uuid>,
    pub mine_only: bool,
}

const ITEM_HEIGHT: u16 = 10;
const THUMB_WIDTH: u16 = 14;
const THUMB_LINES: usize = 6;
const SUMMARY_LINES: usize = 3;

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
    } else {
        let visible_items = ((list_area.height / ITEM_HEIGHT).max(1)) as usize;
        let selected_index = view
            .selected_index
            .min(view.articles.len().saturating_sub(1));
        let start_index = selected_index.saturating_sub(visible_items.saturating_sub(1));
        let end_index = (start_index + visible_items).min(view.articles.len());
        let visible_len = end_index.saturating_sub(start_index);

        let constraints =
            std::iter::repeat_n(Constraint::Length(ITEM_HEIGHT), visible_len).collect::<Vec<_>>();

        let list_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints(constraints)
            .split(list_area);

        for (row, item_area) in list_layout.iter().copied().enumerate() {
            let article_idx = start_index + row;
            let item = &view.articles[article_idx];
            let article = &item.article;
            let is_unread = is_unread(item, view.marker);

            let item_block = Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::default().fg(theme::BORDER()))
                .style(theme::row_style(article_idx == selected_index));

            let content_area = item_block.inner(item_area);
            frame.render_widget(item_block, item_area);

            // Split each item into a Left side (ASCII thumbnail) and Right side (Text)
            let item_split = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Length(THUMB_WIDTH), Constraint::Min(0)])
                .split(content_area);

            let thumb_area = item_split[0];
            let text_area = item_split[1];

            let ascii_lines: Vec<Line> = ascii_preview_if_fit(
                &article.ascii_art,
                (THUMB_WIDTH.saturating_sub(2)) as usize,
                THUMB_LINES,
            )
            .into_iter()
            .map(|line| Line::from(Span::styled(line, Style::default().fg(theme::AMBER_DIM()))))
            .collect();
            let ascii_p = Paragraph::new(ascii_lines);
            frame.render_widget(ascii_p, thumb_area);

            let mut title_spans = Vec::new();
            if is_unread {
                title_spans.push(Span::styled(
                    "● ",
                    Style::default()
                        .fg(theme::AMBER())
                        .add_modifier(Modifier::BOLD),
                ));
            }
            title_spans.push(Span::styled(
                article.title.as_str(),
                Style::default()
                    .fg(theme::TEXT_BRIGHT())
                    .add_modifier(Modifier::BOLD),
            ));

            let mut text_lines = vec![
                Line::from(title_spans),
                Line::from(vec![Span::styled(
                    article.url.as_str(),
                    Style::default()
                        .fg(theme::TEXT_FAINT())
                        .add_modifier(Modifier::ITALIC),
                )]),
                Line::from(vec![
                    Span::styled(
                        format!("@{}", item.author_username),
                        Style::default()
                            .fg(theme::AMBER())
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!(" - {}", format_relative_time(article.created)),
                        Style::default().fg(theme::TEXT_DIM()),
                    ),
                    Span::styled(
                        format!(" - {}", article.created.format("%a %Y-%m-%d %H:%M UTC")),
                        Style::default().fg(theme::TEXT_FAINT()),
                    ),
                    Span::styled(
                        match reading_label(item) {
                            Some(label) => format!(" - {label}"),
                            None => String::new(),
                        },
                        Style::default().fg(theme::TEXT_DIM()),
                    ),
                ]),
            ];

            let summary_clean = article.summary.replace("\\n", "\n");
            let summary_lines: Vec<&str> = summary_clean
                .lines()
                .filter(|line| !line.trim().is_empty())
                .collect();
            for line in summary_lines.iter().take(SUMMARY_LINES).copied() {
                text_lines.push(Line::from(Span::styled(
                    line,
                    Style::default().fg(theme::TEXT()),
                )));
            }
            if summary_lines.len() > SUMMARY_LINES {
                text_lines.push(Line::from(Span::styled(
                    "...",
                    Style::default().fg(theme::TEXT()),
                )));
            }

            let text_p = Paragraph::new(text_lines).wrap(Wrap { trim: true });
            frame.render_widget(text_p, text_area);
        }
    }
}

/// The card meta's last segment: how long the story takes to read, `link`
/// when there is no text to read here (a video, an X post, a page the fetch
/// could not read), nothing while the fetch is pending.
pub(crate) fn reading_label(item: &ArticleFeedItem) -> Option<String> {
    match item.body {
        BodyStatus::Ready { reading_minutes } => Some(format!("{reading_minutes} min read")),
        BodyStatus::Pending => None,
        BodyStatus::Unavailable => Some("link".to_string()),
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
