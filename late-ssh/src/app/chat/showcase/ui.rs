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

use super::state::{ComposerField, State};
use super::svc::ShowcaseFeedItem;

pub struct ShowcaseListView<'a> {
    pub items: &'a [ShowcaseFeedItem],
    pub selected_index: usize,
    pub current_user_id: uuid::Uuid,
    pub is_admin: bool,
    pub marker_read_at: Option<DateTime<Utc>>,
}

const ITEM_HEIGHT: u16 = 7;
const SUMMARY_LINES: usize = 3;

pub fn draw_showcase_list(frame: &mut Frame, area: Rect, view: &ShowcaseListView<'_>) {
    let selected = if view.items.is_empty() {
        0
    } else {
        view.selected_index.min(view.items.len() - 1) + 1
    };
    let title = format!(" Showcases ({selected}/{}) ", view.items.len());
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(theme::BORDER()));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if view.items.is_empty() {
        let text = Text::from(vec![
            Line::from(Span::styled(
                "No showcases yet.",
                Style::default().fg(theme::TEXT_DIM()),
            )),
            Line::from(Span::styled(
                "Press 'i' to share a project link.",
                Style::default().fg(theme::TEXT_DIM()),
            )),
        ]);
        let empty_p = Paragraph::new(text);
        frame.render_widget(empty_p, inner);
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
        let s = &item.showcase;
        let is_selected = item_idx == selected_index;
        let is_unread = view
            .marker_read_at
            .map(|last_read_at| s.created > last_read_at)
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

        let mut lines: Vec<Line> = Vec::new();

        // Title row: title + ownership marker
        let owner = item.showcase.user_id == view.current_user_id;
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
            s.title.as_str(),
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        ));
        if owner {
            title_spans.push(Span::styled(
                "  (yours)",
                Style::default().fg(theme::AMBER_DIM()),
            ));
        } else if view.is_admin {
            title_spans.push(Span::styled(
                "  (admin)",
                Style::default().fg(theme::AMBER_DIM()),
            ));
        }
        lines.push(Line::from(title_spans));

        // URL directly under the title.
        lines.push(Line::from(vec![
            Span::styled("↗ ", Style::default().fg(theme::AMBER_DIM())),
            Span::styled(
                s.url.as_str(),
                Style::default()
                    .fg(theme::TEXT_FAINT())
                    .add_modifier(Modifier::ITALIC),
            ),
        ]));

        // Description (up to 3 visual lines, with inline ellipsis on truncation).
        let (mut desc_lines, truncated) =
            description_summary_lines(&s.description, content_area.width as usize, SUMMARY_LINES);
        if truncated && let Some(last) = desc_lines.last_mut() {
            apply_inline_ellipsis(last, content_area.width as usize);
        }
        for line in desc_lines {
            lines.push(Line::from(Span::styled(
                line,
                Style::default().fg(theme::TEXT()),
            )));
        }

        // Author + time + tags as the footer line.
        let mut meta_spans = vec![
            Span::styled(
                format!("@{}", item.author_username),
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" · {}", format_relative_time(s.created)),
                Style::default().fg(theme::TEXT_DIM()),
            ),
        ];
        if !s.tags.is_empty() {
            let tags_text = s
                .tags
                .iter()
                .map(|t| format!("#{t}"))
                .collect::<Vec<_>>()
                .join(" ");
            meta_spans.push(Span::styled(
                format!("  {tags_text}"),
                Style::default().fg(theme::AMBER_DIM()),
            ));
        }
        lines.push(Line::from(meta_spans));

        let p = Paragraph::new(lines).wrap(Wrap { trim: true });
        frame.render_widget(p, content_area);
    }
}

fn apply_inline_ellipsis(line: &mut String, width: usize) {
    let width = width.max(1);
    let char_count = line.chars().count();
    if char_count < width {
        line.push('…');
        return;
    }
    line.pop();
    line.push('…');
}

fn description_summary_lines(
    description: &str,
    width: usize,
    max_lines: usize,
) -> (Vec<String>, bool) {
    let mut out = Vec::new();
    let mut truncated = false;

    for paragraph in description.lines().filter(|line| !line.trim().is_empty()) {
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

pub struct ShowcaseComposerView<'a> {
    pub state: &'a State,
}

pub fn draw_showcase_composer(frame: &mut Frame, area: Rect, view: &ShowcaseComposerView<'_>) {
    let editing = view.state.editing();
    let composing = view.state.composing();
    let active = view.state.active_field();

    let title = if !composing {
        " Showcase "
    } else if editing {
        " Editing · Tab/S+Tab switch · Enter submit · Alt+Enter newline · Esc cancel "
    } else {
        " New showcase · Tab/S+Tab switch · Enter submit · Alt+Enter newline · Esc cancel "
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
            " j/k navigate · Enter copy URL · i compose · e edit own · d delete own",
            Style::default().fg(theme::TEXT_DIM()),
        )));
        frame.render_widget(hint, inner);
        return;
    }

    // Four-row form: 3 single-line fields then a multi-line description.
    // Description gets the remaining space.
    let constraints = [
        Constraint::Length(2), // title
        Constraint::Length(2), // url
        Constraint::Length(2), // tags
        Constraint::Min(2),    // description
    ];
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner);

    draw_field(frame, rows[0], view.state, ComposerField::Title, active);
    draw_field(frame, rows[1], view.state, ComposerField::Url, active);
    draw_field(frame, rows[2], view.state, ComposerField::Tags, active);
    draw_field(
        frame,
        rows[3],
        view.state,
        ComposerField::Description,
        active,
    );
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
    let label_w: u16 = 18;
    let split = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(label_w),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .split(area);
    let prefix = if is_active { "▸ " } else { "  " };
    let label = Paragraph::new(Line::from(Span::styled(
        format!("{prefix}{}:", field.label()),
        label_style,
    )));
    frame.render_widget(label, split[0]);
    frame.render_widget(Paragraph::new(" "), split[1]);
    if state.field_is_empty(field) {
        draw_empty_placeholder(frame, split[2], field.placeholder(), is_active);
    } else {
        frame.render_widget(state.field_textarea(field), split[2]);
    }
}

fn draw_empty_placeholder(frame: &mut Frame, area: Rect, placeholder: &str, active: bool) {
    let mut chars = placeholder.chars();
    let Some(first) = chars.next() else {
        return;
    };
    let rest = chars.collect::<String>();
    let first = if active {
        Span::styled(
            first.to_string(),
            Style::default()
                .fg(theme::BG_CANVAS())
                .bg(theme::TEXT_DIM())
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled(first.to_string(), Style::default().fg(theme::TEXT_DIM()))
    };
    let line = Line::from(vec![
        first,
        Span::styled(rest, Style::default().fg(theme::TEXT_DIM())),
    ]);
    frame.render_widget(Paragraph::new(line).wrap(Wrap { trim: false }), area);
}

#[cfg(test)]
mod tests {
    use super::description_summary_lines;

    #[test]
    fn description_summary_wraps_to_visual_line_budget() {
        let (lines, truncated) = description_summary_lines("hello wide world\nsecond line", 8, 3);

        assert_eq!(lines, vec!["hello", "wide", "world"]);
        assert!(truncated);
    }

    #[test]
    fn description_summary_preserves_short_multiline_description() {
        let (lines, truncated) = description_summary_lines("one\ntwo\nthree", 20, 3);

        assert_eq!(lines, vec!["one", "two", "three"]);
        assert!(!truncated);
    }
}
