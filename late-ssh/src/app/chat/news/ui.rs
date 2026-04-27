use crate::app::common::primitives::format_relative_time;
use crate::app::common::theme;
use chrono::{DateTime, Utc};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
};

use late_core::models::article::ArticleFeedItem;

pub struct ArticleListView<'a> {
    pub articles: &'a [ArticleFeedItem],
    pub selected_index: usize,
    pub marker_read_at: Option<DateTime<Utc>>,
}

const ITEM_HEIGHT: u16 = 10;
const THUMB_WIDTH: u16 = 14;
const THUMB_LINES: usize = 6;
const SUMMARY_LINES: usize = 3;

pub fn draw_article_list(frame: &mut Frame, area: Rect, view: &ArticleListView<'_>) {
    let selected = if view.articles.is_empty() {
        0
    } else {
        view.selected_index.min(view.articles.len() - 1) + 1
    };
    let title = format!(" News Feed ({selected}/{}) ", view.articles.len());
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(theme::BORDER()));

    let inner_area = block.inner(area);
    frame.render_widget(block, area);

    let list_area = inner_area;

    if view.articles.is_empty() {
        let text = Text::from("No news yet. Press 'i' to share a link.");
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
            let is_unread = view
                .marker_read_at
                .map(|last_read_at| article.created > last_read_at)
                .unwrap_or(true);

            let bg_color = if article_idx == selected_index {
                theme::BG_SELECTION()
            } else {
                Color::Reset
            };

            let item_block = Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::default().fg(theme::BORDER()))
                .style(Style::default().bg(bg_color));

            let content_area = item_block.inner(item_area);
            frame.render_widget(item_block, item_area);

            // Split each item into a Left side (ASCII thumbnail) and Right side (Text)
            let item_split = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Length(THUMB_WIDTH), Constraint::Min(0)])
                .split(content_area);

            let thumb_area = item_split[0];
            let text_area = item_split[1];

            let ascii_art_clean = article.ascii_art.replace("\\n", "\n");
            let ascii_lines: Vec<Line> = raw_ascii_preview_if_fit(
                &ascii_art_clean,
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

fn raw_ascii_preview_if_fit(ascii_art: &str, target_width: usize, max_lines: usize) -> Vec<String> {
    if target_width == 0 || max_lines == 0 {
        return Vec::new();
    }

    let lines: Vec<String> = ascii_art.lines().map(|line| line.to_string()).collect();
    if lines.is_empty() {
        return Vec::new();
    }

    let max_line_width = lines
        .iter()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(0);
    if max_line_width > target_width {
        return Vec::new();
    }

    lines.into_iter().take(max_lines).collect()
}

#[cfg(test)]
mod tests {
    use super::raw_ascii_preview_if_fit;

    #[test]
    fn raw_ascii_preview_keeps_original_lines_when_fit() {
        let input = "abcd\nefgh\nijkl\nmnop";
        let out = raw_ascii_preview_if_fit(input, 4, 2);
        assert_eq!(out, vec!["abcd".to_string(), "efgh".to_string()]);
    }

    #[test]
    fn raw_ascii_preview_hides_art_when_width_too_small() {
        let out = raw_ascii_preview_if_fit("abcdef\n123456", 5, 6);
        assert!(out.is_empty());
    }

    #[test]
    fn raw_ascii_preview_returns_empty_for_empty_input() {
        assert!(raw_ascii_preview_if_fit("", 10, 10).is_empty());
    }

    #[test]
    fn raw_ascii_preview_returns_empty_for_zero_dimensions() {
        assert!(raw_ascii_preview_if_fit("abc", 0, 5).is_empty());
        assert!(raw_ascii_preview_if_fit("abc", 5, 0).is_empty());
    }
}
