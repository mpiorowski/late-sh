use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};
use unicode_width::UnicodeWidthStr;

use late_core::models::chat_message::ChatMessage;

use crate::app::chat::history_modal::state::{ChatHistoryModalState, HistoryStatus, Park};
use crate::app::common::markdown::wrap_plain_line;
use crate::app::common::theme;

const MODAL_WIDTH: u16 = 96;
const MODAL_HEIGHT: u16 = 30;
/// Width of the `HH:MM ` gutter each message opens with; continuation lines
/// of a wrapped body indent by the same amount so bodies stay column-aligned.
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

/// Bodies soft-wrap to the pane rather than truncating, so a message covers a
/// variable number of terminal rows and only a rendered frame knows how many
/// messages make a screenful. The frame writes two things back through the
/// state's `Cell`s: `resolve_viewport` settles any pending park and aligns
/// the scroll index with the first row it actually draws, and
/// `set_visible_rows` reports how many whole messages fit, which drives the
/// paging keys, the bottom-edge test, and the scroll clamp.
fn draw_messages(frame: &mut Frame, area: Rect, state: &ChatHistoryModalState) {
    let messages = state.messages();
    if messages.is_empty() {
        draw_notice(frame, area, "No messages here yet.");
        return;
    }

    let width = area.width as usize;
    let height = (area.height as usize).max(1);
    let body_width = width.saturating_sub(TIME_WIDTH).max(1);

    let start = resolve_viewport(state, height, body_width);
    let divider_before = state.unread_divider_target();
    let mut lines: Vec<Line<'static>> = Vec::with_capacity(height);
    let mut last_date: Option<chrono::NaiveDate> = None;
    let mut fully_shown = 0usize;

    for message in messages.iter().skip(start) {
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
        // The unread divider hugs the first message past the viewer's read
        // cursor, the same rule the live tail draws. Like the day separators
        // it is left out of the viewport budgets: one row off-center is
        // invisible.
        if divider_before == Some(message.id) {
            lines.push(crate::app::chat::ui::new_messages_divider_line(width));
            if lines.len() >= height {
                break;
            }
        }

        let body_style = if state.anchor_id() == Some(message.id) {
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT())
        };
        let time = message.created.format("%H:%M").to_string();
        let mut clipped = false;
        for (row, text) in wrapped_body(state, message, body_width)
            .into_iter()
            .enumerate()
        {
            if lines.len() >= height {
                clipped = true;
                break;
            }
            let gutter = if row == 0 {
                pad_right(&time, TIME_WIDTH)
            } else {
                " ".repeat(TIME_WIDTH)
            };
            lines.push(Line::from(vec![
                Span::styled(gutter, Style::default().fg(theme::TEXT_DIM())),
                Span::styled(text, body_style),
            ]));
        }
        if !clipped {
            fully_shown += 1;
        }
    }

    state.set_visible_rows(fully_shown);
    frame.render_widget(Paragraph::new(lines), area);
}

/// Settle the viewport for this frame and return the first message drawn.
/// A pending park (a fresh open, or End) is resolved now that the pane's
/// line budget is known: `Bottom` lands on the newest message, `Anchor`
/// centers the opened-on message. The scroll index is then aligned with the
/// row actually drawn first, so after a back-fill the next keypress moves
/// from what the user sees instead of dying in the clamp.
fn resolve_viewport(state: &ChatHistoryModalState, height: usize, body_width: usize) -> usize {
    let messages = state.messages();
    match state.take_park() {
        Some(Park::Bottom) => state.sync_scroll_index(messages.len() - 1),
        Some(Park::Anchor) => {
            let anchor_at = state
                .anchor_id()
                .and_then(|id| messages.iter().position(|m| m.id == id))
                .unwrap_or(0);
            state.sync_scroll_index(centered_start(state, anchor_at, height, body_width));
        }
        None => {}
    }
    let start = fill_start(state, height, body_width);
    state.sync_scroll_index(start);
    start
}

/// First message of a window that shows `anchor_at` roughly mid-pane:
/// messages above the anchor are admitted until they cost more than half the
/// pane's lines. Day separators are ignored in the budget; a row off-center
/// is invisible and it keeps the walk one accumulation.
fn centered_start(
    state: &ChatHistoryModalState,
    anchor_at: usize,
    height: usize,
    body_width: usize,
) -> usize {
    let budget = height / 2;
    let mut used = 0usize;
    let mut start = anchor_at;
    while start > 0 {
        let above = wrapped_body(state, &state.messages()[start - 1], body_width).len();
        if used + above > budget {
            break;
        }
        used += above;
        start -= 1;
    }
    start
}

/// First message the pane draws. Normally the scroll index itself; when the
/// window from there runs out of messages before the pane is full (the
/// viewport is parked at the tail), it walks back so the pane fills from the
/// bottom up instead of showing the newest message alone above blank space.
fn fill_start(state: &ChatHistoryModalState, height: usize, body_width: usize) -> usize {
    let messages = state.messages();
    let mut start = state.scroll_index().min(messages.len() - 1);
    while start > 0 && window_lines(state, start - 1, height, body_width) <= height {
        start -= 1;
    }
    start
}

/// Lines the window starting at `start` would fill, stopping as soon as it
/// passes `height`: `fill_start` only needs to know "fits" or "overflows".
fn window_lines(
    state: &ChatHistoryModalState,
    start: usize,
    height: usize,
    body_width: usize,
) -> usize {
    let mut total = 0usize;
    let mut last_date: Option<chrono::NaiveDate> = None;
    for message in state.messages().iter().skip(start) {
        let date = message.created.date_naive();
        if last_date != Some(date) {
            last_date = Some(date);
            total += 1;
        }
        total += wrapped_body(state, message, body_width).len();
        if total > height {
            break;
        }
    }
    total
}

/// A message's rendered lines: `author: body`, soft-wrapped to the pane.
fn wrapped_body(state: &ChatHistoryModalState, message: &ChatMessage, width: usize) -> Vec<String> {
    let author = state
        .usernames()
        .get(&message.user_id)
        .map(String::as_str)
        .unwrap_or("?");
    let text = format!("{author}: {}", message.body.replace('\r', ""));
    let mut out: Vec<String> = text
        .lines()
        .flat_map(|line| wrap_plain_line(line, width))
        .collect();
    // `wrap_plain_line` drops blank text; a whitespace-only body still needs
    // one row so the message doesn't vanish from the run.
    if out.is_empty() {
        out.push(String::new());
    }
    out
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

fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    let [row] = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .areas(area);
    let [cell] = Layout::horizontal([Constraint::Length(width)])
        .flex(Flex::Center)
        .areas(row);
    cell
}

#[cfg(test)]
#[path = "ui_test.rs"]
mod ui_test;
