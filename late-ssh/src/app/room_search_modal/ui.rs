use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Margin, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::app::{
    chat::state::ChatState,
    common::theme,
    room_search_modal::state::{RoomSearchModalState, filtered_items},
};
use uuid::Uuid;

const MODAL_WIDTH: u16 = 62;
const MODAL_HEIGHT: u16 = 18;

pub(crate) fn draw(
    frame: &mut Frame,
    area: Rect,
    state: &RoomSearchModalState,
    chat: &ChatState,
    user_id: Uuid,
) {
    let popup = centered_rect(
        area,
        MODAL_WIDTH.min(area.width),
        MODAL_HEIGHT.min(area.height),
    );
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(" Jump To Room ")
        .title_style(
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    if inner.height < 5 || inner.width < 20 {
        frame.render_widget(Paragraph::new("Terminal too small"), inner);
        return;
    }

    let layout = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .split(inner);

    draw_query(frame, layout[0], state);
    draw_results(frame, layout[1], state, chat, user_id);
    draw_footer(frame, layout[2]);
}

fn draw_query(frame: &mut Frame, area: Rect, state: &RoomSearchModalState) {
    let block = Block::default()
        .title(" Search ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
    let inner = block.inner(area).inner(Margin {
        vertical: 0,
        horizontal: 1,
    });
    frame.render_widget(block, area);

    let mut query = state.query().to_string();
    query.push('█');
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            query,
            Style::default().fg(theme::TEXT_BRIGHT()),
        ))),
        inner,
    );
}

fn draw_results(
    frame: &mut Frame,
    area: Rect,
    state: &RoomSearchModalState,
    chat: &ChatState,
    user_id: Uuid,
) {
    let items = filtered_items(chat, user_id, state.query());
    let selected = state.selected();
    if items.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "  No matching rooms",
                Style::default().fg(theme::TEXT_DIM()),
            ))),
            area,
        );
        return;
    }

    let height = area.height as usize;
    let start = selected
        .saturating_sub(height.saturating_sub(1))
        .min(items.len().saturating_sub(height));
    let width = area.width as usize;
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut last_section: Option<bool> = None;
    for (index, item) in items.iter().enumerate().skip(start) {
        if lines.len() >= height {
            break;
        }
        if last_section != Some(item.favorite) {
            last_section = Some(item.favorite);
            let label = if item.favorite { "favorites" } else { "rooms" };
            lines.push(Line::from(Span::styled(
                format!("  {label}"),
                Style::default()
                    .fg(theme::TEXT_FAINT())
                    .add_modifier(Modifier::ITALIC),
            )));
            if lines.len() >= height {
                break;
            }
        }
        let active = index == selected;
        let marker = if active { ">" } else { " " };
        let style = if active {
            Style::default()
                .fg(theme::AMBER_GLOW())
                .bg(theme::BG_SELECTION())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT())
        };
        let meta_style = if active {
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .bg(theme::BG_SELECTION())
        } else {
            Style::default().fg(theme::TEXT_DIM())
        };
        let unread = if item.unread_count > 0 {
            item.unread_count.to_string()
        } else {
            String::new()
        };
        let unread_style = if item.unread_count > 0 {
            let base = Style::default().fg(theme::AMBER_GLOW());
            if active {
                base.bg(theme::BG_SELECTION())
            } else {
                base
            }
        } else {
            meta_style
        };
        let unread_width = 6usize.min(width.saturating_sub(6));
        let meta_width = 16usize.min(width.saturating_sub(unread_width + 6));
        let label_width = width.saturating_sub(meta_width + unread_width + 5);
        lines.push(Line::from(vec![
            Span::styled(format!("{marker} "), style),
            Span::styled(
                pad_right(&truncate_to_width(&item.label, label_width), label_width),
                style,
            ),
            Span::styled(" ", style),
            Span::styled(
                pad_right(&truncate_to_width(&item.meta, meta_width), meta_width),
                meta_style,
            ),
            Span::styled(" ", style),
            Span::styled(
                pad_left(&truncate_to_width(&unread, unread_width), unread_width),
                unread_style,
            ),
        ]));
    }

    frame.render_widget(Paragraph::new(lines), area);
}

fn draw_footer(frame: &mut Frame, area: Rect) {
    let line = Line::from(vec![
        Span::styled("Enter", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" jump  ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("↑↓", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" select  ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("Esc", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" close", Style::default().fg(theme::TEXT_DIM())),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    let vertical = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .split(area);
    let horizontal = Layout::horizontal([Constraint::Length(width)])
        .flex(Flex::Center)
        .split(vertical[0]);
    horizontal[0]
}

fn pad_right(text: &str, width: usize) -> String {
    let used = UnicodeWidthStr::width(text);
    let mut out = String::with_capacity(text.len() + width.saturating_sub(used));
    out.push_str(text);
    out.push_str(&" ".repeat(width.saturating_sub(used)));
    out
}

fn pad_left(text: &str, width: usize) -> String {
    let used = UnicodeWidthStr::width(text);
    let mut out = String::with_capacity(text.len() + width.saturating_sub(used));
    out.push_str(&" ".repeat(width.saturating_sub(used)));
    out.push_str(text);
    out
}

fn truncate_to_width(text: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    if UnicodeWidthStr::width(text) <= width {
        return text.to_string();
    }
    if width == 1 {
        return "…".to_string();
    }
    let mut out = String::new();
    let mut used = 0usize;
    for ch in text.chars() {
        let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + ch_width >= width {
            break;
        }
        out.push(ch);
        used += ch_width;
    }
    out.push('…');
    out
}
