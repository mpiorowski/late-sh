use crate::app::common::theme;
use crate::app::common::{composer, primitives::format_relative_time};
use chrono::{DateTime, Utc};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
};

use super::state::{ComposerField, State, status_label};
use super::svc::WorkFeedItem;

pub struct WorkListView<'a> {
    pub items: &'a [WorkFeedItem],
    pub selected_index: usize,
    pub current_user_id: uuid::Uuid,
    pub is_admin: bool,
    pub marker_read_at: Option<DateTime<Utc>>,
}

const ITEM_HEIGHT: u16 = 8;
const SUMMARY_LINES: usize = 2;

pub fn draw_work_list(frame: &mut Frame, area: Rect, view: &WorkListView<'_>) {
    let selected = if view.items.is_empty() {
        0
    } else {
        view.selected_index.min(view.items.len() - 1) + 1
    };
    let title = format!(" Work ({selected}/{}) ", view.items.len());
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(theme::BORDER()));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if view.items.is_empty() {
        let text = Text::from(vec![
            Line::from(Span::styled(
                "No work profiles yet.",
                Style::default().fg(theme::TEXT_DIM()),
            )),
            Line::from(Span::styled(
                "Press 'i' to create yours.",
                Style::default().fg(theme::TEXT_DIM()),
            )),
        ]);
        frame.render_widget(Paragraph::new(text), inner);
        return;
    }

    let visible_items = ((inner.height / ITEM_HEIGHT).max(1)) as usize;
    let selected_index = view.selected_index.min(view.items.len().saturating_sub(1));
    let start_index = selected_index.saturating_sub(visible_items.saturating_sub(1));
    let end_index = (start_index + visible_items).min(view.items.len());
    let visible_len = end_index.saturating_sub(start_index);

    let constraints =
        std::iter::repeat_n(Constraint::Length(ITEM_HEIGHT), visible_len).collect::<Vec<_>>();
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner);

    for (row, item_area) in layout.iter().copied().enumerate() {
        let item_idx = start_index + row;
        let item = &view.items[item_idx];
        let p = &item.profile;
        let is_selected = item_idx == selected_index;
        let is_unread = view
            .marker_read_at
            .map(|last_read_at| p.updated > last_read_at)
            .unwrap_or(true);
        let bg = if is_selected {
            theme::BG_SELECTION()
        } else {
            Color::Reset
        };

        let item_block = Block::default()
            .borders(Borders::BOTTOM)
            .border_style(Style::default().fg(theme::BORDER()))
            .style(Style::default().bg(bg));
        let content_area = item_block.inner(item_area);
        frame.render_widget(item_block, item_area);

        let owner = p.user_id == view.current_user_id;
        let mut title_spans = Vec::new();
        if is_unread {
            title_spans.push(Span::styled(
                "* ",
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            ));
        }
        title_spans.push(Span::styled(
            p.headline.as_str(),
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        ));
        if owner {
            title_spans.push(Span::styled(
                "  (yours)",
                Style::default().fg(theme::AMBER_DIM()),
            ));
        }

        let mut lines = vec![
            Line::from(title_spans),
            Line::from(vec![
                Span::styled("@", Style::default().fg(theme::TEXT_DIM())),
                Span::styled(
                    item.author_username.as_str(),
                    Style::default()
                        .fg(theme::AMBER())
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(
                        " - {} - {} - {}",
                        status_label(&p.status),
                        p.work_type,
                        p.location
                    ),
                    Style::default().fg(theme::TEXT_DIM()),
                ),
            ]),
        ];

        let (mut summary_lines, truncated) =
            summary_lines(&p.summary, content_area.width as usize, SUMMARY_LINES);
        if truncated && let Some(last) = summary_lines.last_mut() {
            apply_inline_ellipsis(last, content_area.width as usize);
        }
        for line in summary_lines {
            lines.push(Line::from(Span::styled(
                line,
                Style::default().fg(theme::TEXT()),
            )));
        }

        if !p.skills.is_empty() {
            lines.push(Line::from(Span::styled(
                p.skills
                    .iter()
                    .map(|skill| format!("#{skill}"))
                    .collect::<Vec<_>>()
                    .join(" "),
                Style::default().fg(theme::AMBER_DIM()),
            )));
        }

        let first_link = p.links.first().map(String::as_str).unwrap_or("");
        lines.push(Line::from(vec![
            Span::styled("link ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled(
                first_link.to_string(),
                Style::default()
                    .fg(theme::TEXT_FAINT())
                    .add_modifier(Modifier::ITALIC),
            ),
            Span::styled(
                format!(" - {} - {}", p.slug, format_relative_time(p.updated)),
                Style::default().fg(theme::TEXT_DIM()),
            ),
        ]));

        frame.render_widget(
            Paragraph::new(lines).wrap(Wrap { trim: true }),
            content_area,
        );
    }
}

fn apply_inline_ellipsis(line: &mut String, width: usize) {
    let width = width.max(1);
    if line.chars().count() < width {
        line.push_str("...");
        return;
    }
    line.pop();
    line.push('.');
}

fn summary_lines(summary: &str, width: usize, max_lines: usize) -> (Vec<String>, bool) {
    let mut out = Vec::new();
    let mut truncated = false;
    for paragraph in summary.lines().filter(|line| !line.trim().is_empty()) {
        let wrapped = composer::build_composer_rows(paragraph.trim(), width.max(1));
        let rows: Vec<String> = if wrapped.is_empty() {
            vec![String::new()]
        } else {
            wrapped.into_iter().map(|row| row.text).collect()
        };
        for row in rows {
            if out.len() == max_lines {
                truncated = true;
                break;
            }
            out.push(row);
        }
        if truncated {
            break;
        }
    }
    (out, truncated)
}

pub struct WorkComposerView<'a> {
    pub state: &'a State,
}

pub fn draw_work_composer(frame: &mut Frame, area: Rect, view: &WorkComposerView<'_>) {
    let editing = view.state.editing();
    let composing = view.state.composing();
    let active = view.state.active_field();

    let title = if !composing {
        " Work "
    } else if editing {
        " Editing work profile - Tab/S+Tab switch - Enter submit - Alt+Enter newline - Esc cancel "
    } else {
        " New work profile - Tab/S+Tab switch - Enter submit - Alt+Enter newline - Esc cancel "
    };
    let border_style = if composing {
        Style::default().fg(theme::BORDER_ACTIVE())
    } else {
        Style::default().fg(theme::BORDER())
    };
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(border_style);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if !composing {
        let hint = Paragraph::new(Line::from(Span::styled(
            " j/k navigate - Enter/c copy profile - i create/edit yours - e edit selected - d delete own",
            Style::default().fg(theme::TEXT_DIM()),
        )));
        frame.render_widget(hint, inner);
        return;
    }

    let constraints = [
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(2),
    ];
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner);

    draw_field(frame, rows[0], view.state, ComposerField::Headline, active);
    draw_field(frame, rows[1], view.state, ComposerField::Status, active);
    draw_field(frame, rows[2], view.state, ComposerField::Type, active);
    draw_field(frame, rows[3], view.state, ComposerField::Location, active);
    draw_field(frame, rows[4], view.state, ComposerField::Links, active);
    draw_field(frame, rows[5], view.state, ComposerField::Skills, active);
    draw_field(frame, rows[6], view.state, ComposerField::Includes, active);
    draw_field(frame, rows[7], view.state, ComposerField::Summary, active);
}

fn draw_field(
    frame: &mut Frame,
    area: Rect,
    state: &State,
    field: ComposerField,
    active: ComposerField,
) {
    let is_active = field == active;
    let label_style = if is_active {
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_DIM())
    };
    let label_w: u16 = 14;
    let split = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(label_w),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .split(area);
    let prefix = if is_active { "> " } else { "  " };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!("{prefix}{}:", field.label()),
            label_style,
        ))),
        split[0],
    );
    frame.render_widget(Paragraph::new(" "), split[1]);
    frame.render_widget(state.field_textarea(field), split[2]);
}

#[cfg(test)]
mod tests {
    use super::summary_lines;

    #[test]
    fn summary_lines_wrap_to_budget() {
        let (lines, truncated) = summary_lines("hello wide world", 8, 2);
        assert_eq!(lines, vec!["hello", "wide"]);
        assert!(truncated);
    }
}
