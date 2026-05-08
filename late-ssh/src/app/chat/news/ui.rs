use crate::app::chat::ui_text::{NewsPayload, format_news_ascii_art_for_display};
use crate::app::common::primitives::format_relative_time;
use crate::app::common::theme;
use chrono::{DateTime, Utc};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Flex, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use late_core::models::article::ArticleFeedItem;

pub struct ArticleListView<'a> {
    pub articles: &'a [ArticleFeedItem],
    pub selected_index: usize,
    pub marker_read_at: Option<DateTime<Utc>>,
    pub mine_only: bool,
}

pub(crate) struct ArticleModalView<'a> {
    pub payload: &'a NewsPayload,
    pub meta: &'a str,
}

const ITEM_HEIGHT: u16 = 10;
const THUMB_WIDTH: u16 = 14;
const THUMB_LINES: usize = 6;
const SUMMARY_LINES: usize = 3;
const MODAL_SUMMARY_BULLETS: usize = 3;
const MODAL_SUMMARY_LINES_PER_BULLET: usize = 2;
const MODAL_MAX_WIDTH: u16 = 160;
const MODAL_MIN_WIDTH: u16 = 24;

pub fn draw_article_list(frame: &mut Frame, area: Rect, view: &ArticleListView<'_>) {
    let selected = if view.articles.is_empty() {
        0
    } else {
        view.selected_index.min(view.articles.len() - 1) + 1
    };
    let title = if view.mine_only {
        format!(" News Feed · mine ({selected}/{}) ", view.articles.len())
    } else {
        format!(" News Feed ({selected}/{}) ", view.articles.len())
    };
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

pub(crate) fn draw_article_modal(frame: &mut Frame, area: Rect, view: ArticleModalView<'_>) {
    if area.width < MODAL_MIN_WIDTH || area.height < 5 {
        return;
    }

    let popup_width = area
        .width
        .saturating_sub(2)
        .clamp(MODAL_MIN_WIDTH, MODAL_MAX_WIDTH)
        .min(area.width);
    let content_width = popup_width.saturating_sub(4) as usize;
    let content_lines = build_article_modal_lines(&view, content_width);
    let popup_height = (content_lines.len() as u16 + 2).min(area.height).max(5);
    let popup = centered_rect(popup_width, popup_height, area);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(" News Item ")
        .title_bottom(Line::from(vec![
            Span::styled(" Enter", Style::default().fg(theme::AMBER_DIM())),
            Span::styled(" copy link", Style::default().fg(theme::TEXT_DIM())),
            Span::styled(" ── ", Style::default().fg(theme::BORDER())),
            Span::styled("N", Style::default().fg(theme::AMBER_DIM())),
            Span::styled(" open in News", Style::default().fg(theme::TEXT_DIM())),
            Span::styled(" ── ", Style::default().fg(theme::BORDER())),
            Span::styled("Esc", Style::default().fg(theme::AMBER_DIM())),
            Span::styled(" close ", Style::default().fg(theme::TEXT_DIM())),
        ]))
        .title_style(
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let content = inner.inner(Margin {
        horizontal: 1,
        vertical: 0,
    });
    frame.render_widget(Paragraph::new(content_lines), content);
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

fn build_article_modal_lines(view: &ArticleModalView<'_>, width: usize) -> Vec<Line<'static>> {
    let title_style = Style::default()
        .fg(theme::TEXT_BRIGHT())
        .add_modifier(Modifier::BOLD);
    let url_style = Style::default()
        .fg(theme::TEXT_FAINT())
        .add_modifier(Modifier::ITALIC);
    let meta_style = Style::default().fg(theme::TEXT_DIM());
    let body_style = Style::default().fg(theme::TEXT());
    let art_style = Style::default().fg(theme::AMBER_DIM());

    let (left_width, gap_width, right_width) = modal_columns(width);
    let title = normalize_inline_text(&view.payload.title);
    let url = normalize_inline_text(&view.payload.url);
    let mut right_rows = Vec::new();

    for row in wrap_plain_display_width(
        if title.is_empty() {
            "news update"
        } else {
            title.as_str()
        },
        right_width,
    ) {
        right_rows.push((row, title_style));
    }
    if !url.is_empty() {
        for row in wrap_plain_display_width(&url, right_width) {
            right_rows.push((row, url_style));
        }
    }
    if !view.meta.is_empty() {
        for row in wrap_plain_display_width(view.meta, right_width) {
            right_rows.push((row, meta_style));
        }
    }
    for bullet in split_summary_bullets(&view.payload.summary)
        .into_iter()
        .take(MODAL_SUMMARY_BULLETS)
    {
        for row in wrap_plain_display_width(&bullet, right_width)
            .into_iter()
            .take(MODAL_SUMMARY_LINES_PER_BULLET)
        {
            right_rows.push((row, body_style));
        }
    }

    let ascii_lines = if left_width == 0 {
        Vec::new()
    } else {
        ascii_preview_if_fit(&view.payload.ascii_art, left_width, THUMB_LINES)
    };
    let row_count = ascii_lines.len().max(right_rows.len()).max(1);

    let mut lines = Vec::with_capacity(row_count + 2);
    lines.push(Line::default());
    for idx in 0..row_count {
        let left = ascii_lines.get(idx).map(String::as_str).unwrap_or("");
        let (right, right_style) = right_rows
            .get(idx)
            .map(|(text, style)| (text.as_str(), *style))
            .unwrap_or(("", body_style));
        lines.push(article_modal_row(
            left,
            left_width,
            gap_width,
            right,
            right_style,
            art_style,
        ));
    }
    lines.push(Line::default());
    lines
}

fn modal_columns(width: usize) -> (usize, usize, usize) {
    let left_width = THUMB_WIDTH as usize;
    let gap_width = 2;
    if width >= left_width + gap_width + 12 {
        (left_width, gap_width, width - left_width - gap_width)
    } else {
        (0, 0, width.max(1))
    }
}

fn article_modal_row(
    left: &str,
    left_width: usize,
    gap_width: usize,
    right: &str,
    right_style: Style,
    art_style: Style,
) -> Line<'static> {
    if left_width == 0 {
        return Line::from(Span::styled(right.to_string(), right_style));
    }

    Line::from(vec![
        Span::styled(pad_to_display_width(left, left_width), art_style),
        Span::raw(" ".repeat(gap_width)),
        Span::styled(right.to_string(), right_style),
    ])
}

fn normalize_inline_text(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn split_summary_bullets(text: &str) -> Vec<String> {
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

fn wrap_plain_display_width(text: &str, width: usize) -> Vec<String> {
    if text.trim().is_empty() {
        return Vec::new();
    }
    if width == 0 {
        return vec![String::new()];
    }

    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut idx = 0;
    while idx < chars.len() {
        let mut end = idx;
        let mut used = 0;
        while end < chars.len() {
            let ch_width = UnicodeWidthChar::width(chars[end]).unwrap_or(0);
            if used > 0 && used + ch_width > width {
                break;
            }
            used += ch_width;
            end += 1;
            if used >= width {
                break;
            }
        }

        let break_at = if end < chars.len() {
            let mut pos = end;
            while pos > idx && chars[pos - 1] != ' ' {
                pos -= 1;
            }
            if pos > idx { pos } else { end.max(idx + 1) }
        } else {
            end
        };
        out.push(chars[idx..break_at].iter().collect());
        idx = break_at;
        while idx < chars.len() && chars[idx] == ' ' {
            idx += 1;
        }
    }
    out
}

fn pad_to_display_width(text: &str, width: usize) -> String {
    let mut out = String::new();
    let mut used = 0;
    for ch in text.chars() {
        let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + ch_width > width {
            break;
        }
        out.push(ch);
        used += ch_width;
    }
    out.push_str(&" ".repeat(width.saturating_sub(used)));
    out
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([Constraint::Length(height.min(area.height))])
        .flex(Flex::Center)
        .split(area);
    let horizontal = Layout::horizontal([Constraint::Length(width.min(area.width))])
        .flex(Flex::Center)
        .split(vertical[0]);
    horizontal[0]
}

#[cfg(test)]
mod tests {
    use super::{ArticleModalView, ascii_preview_if_fit, build_article_modal_lines};
    use crate::app::chat::ui_text::NewsPayload;
    use ratatui::text::Line;
    use unicode_width::UnicodeWidthStr;

    fn lines_to_strings(lines: &[Line]) -> Vec<String> {
        lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect()
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

    #[test]
    fn article_modal_lines_use_feed_style_without_news_emoji() {
        let payload = NewsPayload {
            title: "Nobody understands the point of hybrid cars".to_string(),
            summary: "YouTube video by Technology Connections.\nOpen the link to watch on YouTube."
                .to_string(),
            url: "https://www.youtube.com/watch?v=KnUFH5GX_fI".to_string(),
            ascii_art: ".. .-:::----\n. .:==-.....\n:-:--:     .\n-===---:   :\n      .."
                .to_string(),
        };
        let view = ArticleModalView {
            payload: &payload,
            meta: "@artboard - 12 mins ago - Wed 2026-05-06 20:12 UTC",
        };
        let rendered = lines_to_strings(&build_article_modal_lines(&view, 100));

        assert_eq!(rendered.first().map(String::as_str), Some(""));
        assert_eq!(rendered.last().map(String::as_str), Some(""));
        assert!(!rendered.join("\n").contains('📰'));
        assert!(rendered[1].contains(".. .-:::----"));
        assert!(rendered[1].contains("Nobody understands the point of hybrid cars"));
        assert!(rendered[2].contains("https://www.youtube.com/watch?v=KnUFH5GX_fI"));
        assert!(rendered[3].contains("@artboard - 12 mins ago - Wed 2026-05-06 20:12 UTC"));
    }

    #[test]
    fn article_modal_lines_respect_terminal_cell_width() {
        let payload = NewsPayload {
            title: "Nobody understands the point of hybrid cars".to_string(),
            summary: "YouTube video by Technology Connections.\nOpen the link to watch on YouTube."
                .to_string(),
            url: "https://www.youtube.com/watch?v=KnUFH5GX_fI".to_string(),
            ascii_art: ".. .-:::----\n. .:==-.....\n:-:--:     .".to_string(),
        };
        let view = ArticleModalView {
            payload: &payload,
            meta: "@artboard - 12 mins ago - Wed 2026-05-06 20:12 UTC",
        };
        let width = 58;

        for rendered in lines_to_strings(&build_article_modal_lines(&view, width)) {
            assert!(
                UnicodeWidthStr::width(rendered.as_str()) <= width,
                "line overflowed {width} cells: {rendered:?}"
            );
        }
    }

    #[test]
    fn article_modal_lines_tolerate_less_than_six_art_rows() {
        let payload = NewsPayload {
            title: "Tiny art".to_string(),
            summary: "One summary line.".to_string(),
            url: "https://example.com".to_string(),
            ascii_art: "\n  *  \n\n".to_string(),
        };
        let view = ArticleModalView {
            payload: &payload,
            meta: "@artboard - now - Wed 2026-05-06 20:12 UTC",
        };
        let rendered = lines_to_strings(&build_article_modal_lines(&view, 80)).join("\n");

        assert!(rendered.contains("  *"));
        assert!(rendered.contains("Tiny art"));
        assert!(rendered.contains("One summary line."));
    }

    #[test]
    fn article_modal_lines_expand_each_summary_bullet_to_two_rows() {
        let payload = NewsPayload {
            title: "Modal expansion".to_string(),
            summary: [
                "First bullet has enough words to wrap into a second visible row in the modal.",
                "Second bullet also has enough words to use two visible rows in the modal.",
                "Third bullet should still appear with the same two row budget.",
                "Fourth bullet should not appear because the modal caps summary bullets.",
            ]
            .join("\n"),
            url: "https://example.com/news".to_string(),
            ascii_art: String::new(),
        };
        let view = ArticleModalView {
            payload: &payload,
            meta: "",
        };
        let rendered = lines_to_strings(&build_article_modal_lines(&view, 48));
        let body = rendered.join("\n");

        assert!(body.contains("First bullet"));
        assert!(body.contains("second visible"));
        assert!(body.contains("Second bullet"));
        assert!(body.contains("two visible"));
        assert!(body.contains("Third bullet"));
        assert!(!body.contains("Fourth bullet"));
    }
}
