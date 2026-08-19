use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::app::chat::history_modal::state::{ChatHistoryModalState, HistoryStatus};
use crate::app::common::theme;

const MODAL_WIDTH: u16 = 96;
const MODAL_HEIGHT: u16 = 30;
/// Width of the `HH:MM ` gutter each row opens with.
const TIME_WIDTH: usize = 6;

pub(crate) fn draw(frame: &mut Frame, area: Rect, state: &ChatHistoryModalState) {
    let popup = centered_rect(
        area,
        MODAL_WIDTH.min(area.width.saturating_sub(4)).max(20),
        MODAL_HEIGHT.min(area.height.saturating_sub(2)).max(5),
    );
    frame.render_widget(Clear, popup);

    let title = format!(" History · {} ", state.room_label());
    let block = Block::default()
        .title(title)
        .title_style(
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let [body, footer] = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(inner);

    // The renderer is the only place that knows how many rows fit, so it
    // tells the state; scrolling and the bottom-edge test both read it back.
    state.set_visible_rows(body.height as usize);

    match state.status() {
        HistoryStatus::Loading => draw_notice(frame, body, "Loading history…"),
        HistoryStatus::AnchorMissing => {
            draw_notice(frame, body, "That message is no longer available.")
        }
        HistoryStatus::Failed => draw_notice(frame, body, "Could not load history."),
        HistoryStatus::Ready => draw_messages(frame, body, state),
    }
    draw_footer(frame, footer, state);
}

fn draw_notice(frame: &mut Frame, area: Rect, text: &str) {
    let line = Line::from(Span::styled(
        text.to_string(),
        Style::default().fg(theme::TEXT_DIM()),
    ));
    frame.render_widget(Paragraph::new(vec![line]), area);
}

fn draw_messages(frame: &mut Frame, area: Rect, state: &ChatHistoryModalState) {
    let messages = state.messages();
    if messages.is_empty() {
        draw_notice(frame, area, "No messages here yet.");
        return;
    }

    let width = area.width as usize;
    let height = area.height as usize;
    let anchor_id = state.anchor_id();
    let mut lines: Vec<Line<'static>> = Vec::with_capacity(height);
    let mut last_date: Option<chrono::NaiveDate> = None;

    for message in messages.iter().skip(state.scroll_index()) {
        if lines.len() >= height {
            break;
        }
        // A day separator whenever the date turns over, so a long scroll back
        // stays locatable in time rather than becoming an undated wall.
        let date = message.created.date_naive();
        if last_date != Some(date) {
            last_date = Some(date);
            lines.push(Line::from(Span::styled(
                format!("── {date} "),
                Style::default().fg(theme::BORDER_DIM()),
            )));
            if lines.len() >= height {
                break;
            }
        }

        let author = state
            .usernames()
            .get(&message.user_id)
            .cloned()
            .unwrap_or_else(|| "?".to_string());
        let body = message.body.replace(['\n', '\r'], " ");
        let text = format!("{author}: {body}");
        let is_anchor = anchor_id == Some(message.id);
        let body_style = if is_anchor {
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT())
        };

        let time = message.created.format("%H:%M").to_string();
        lines.push(Line::from(vec![
            Span::styled(
                pad_right(&time, TIME_WIDTH),
                Style::default().fg(theme::TEXT_DIM()),
            ),
            Span::styled(
                truncate_to_width(&text, width.saturating_sub(TIME_WIDTH)),
                body_style,
            ),
        ]));
    }

    frame.render_widget(Paragraph::new(lines), area);
}

fn draw_footer(frame: &mut Frame, area: Rect, state: &ChatHistoryModalState) {
    let mut spans = vec![
        Span::styled("↑↓/PgUp/PgDn", Style::default().fg(theme::AMBER_GLOW())),
        Span::styled(" scroll  ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("Esc", Style::default().fg(theme::AMBER_GLOW())),
        Span::styled(" close", Style::default().fg(theme::TEXT_DIM())),
    ];
    if state.is_fetching() {
        spans.push(Span::styled(
            "   loading…",
            Style::default().fg(theme::TEXT_DIM()),
        ));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn pad_right(text: &str, width: usize) -> String {
    let mut out = text.to_string();
    let text_width = UnicodeWidthStr::width(text);
    if text_width < width {
        out.push_str(&" ".repeat(width - text_width));
    }
    out
}

fn truncate_to_width(text: &str, width: usize) -> String {
    if UnicodeWidthStr::width(text) <= width {
        return text.to_string();
    }
    let mut out = String::new();
    let mut used = 0usize;
    for ch in text.chars() {
        let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + ch_width > width.saturating_sub(1) {
            break;
        }
        out.push(ch);
        used += ch_width;
    }
    out.push('…');
    out
}

fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    let [row] = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .areas(area);
    let [cell] = Layout::horizontal([Constraint::Length(width)])
        .flex(Flex::Center)
        .areas(row);
    cell
}
