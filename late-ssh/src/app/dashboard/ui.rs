use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::app::{
    chat::ui::{DashboardChatView, draw_dashboard_chat_card},
    common::{markdown::wrap_plain_line, theme},
    files::terminal_image::TerminalImageFrame,
};
use late_core::models::chat_message::ChatMessage;

pub struct DashboardRenderInput<'a> {
    pub pinned_messages: &'a [ChatMessage],
    pub chat_view: DashboardChatView<'a>,
}

/// Page-1 Home surface: pinned messages (when any) above the selected room's
/// chat. Non-lounge rooms bypass this and render as full chat in `render.rs`.
pub fn draw_dashboard(
    frame: &mut Frame,
    area: Rect,
    view: DashboardRenderInput<'_>,
    terminal_images: &mut TerminalImageFrame,
) {
    if area.width == 0 || area.height == 0 {
        draw_dashboard_chat_card(frame, area, view.chat_view, terminal_images);
        return;
    }

    let pinned_height = dashboard_pinned_height(area.height, area.width, view.pinned_messages);
    if pinned_height == 0 {
        draw_dashboard_chat_card(frame, area, view.chat_view, terminal_images);
        return;
    }

    let [pinned_area, rule_area, chat_area] = Layout::vertical([
        Constraint::Length(pinned_height),
        Constraint::Length(CHAT_RULE_HEIGHT),
        Constraint::Fill(1),
    ])
    .areas(area);

    draw_pinned_messages(frame, pinned_area, view.pinned_messages);
    draw_amber_rule(frame, rule_area);
    draw_dashboard_chat_card(frame, chat_area, view.chat_view, terminal_images);
}

const MAX_PINNED_HEIGHT: u16 = 6;
const CHAT_RULE_HEIGHT: u16 = 1;
pub(crate) const MIN_CHAT_HEIGHT_WITH_LOUNGE: u16 = 10;
const PINNED_GLYPH: &str = "● ";

/// Rows the pinned strip gets, or 0 when there are no pins or the chat area
/// would drop below its minimum.
fn dashboard_pinned_height(height: u16, width: u16, pinned_messages: &[ChatMessage]) -> u16 {
    let pinned_height = pinned_natural_height(pinned_messages, width);
    if pinned_height == 0 {
        return 0;
    }
    if pinned_height + CHAT_RULE_HEIGHT + MIN_CHAT_HEIGHT_WITH_LOUNGE > height {
        return 0;
    }
    pinned_height
}

/// Pre-wrap pinned messages to `width` and return the Lines, ready to render.
/// Same pattern chat uses: split into Lines, count Lines, render Lines.
fn pinned_lines(messages: &[ChatMessage], width: u16) -> Vec<Line<'static>> {
    if width == 0 {
        return Vec::new();
    }
    let prefix_w = PINNED_GLYPH.chars().count();
    let body_w = (width as usize).saturating_sub(prefix_w);
    if body_w == 0 {
        return Vec::new();
    }
    let indent = " ".repeat(prefix_w);
    let mut lines: Vec<Line<'static>> = Vec::new();
    for msg in messages {
        let flat: String = msg.body.split_whitespace().collect::<Vec<_>>().join(" ");
        let wraps = wrap_plain_line(&flat, body_w);
        let wraps = if wraps.is_empty() {
            vec![String::new()]
        } else {
            wraps
        };
        for (idx, chunk) in wraps.into_iter().enumerate() {
            let line = if idx == 0 {
                Line::from(vec![
                    Span::styled(PINNED_GLYPH, Style::default().fg(theme::AMBER())),
                    Span::styled(chunk, Style::default().fg(theme::TEXT())),
                ])
            } else {
                Line::from(vec![
                    Span::raw(indent.clone()),
                    Span::styled(chunk, Style::default().fg(theme::TEXT())),
                ])
            };
            lines.push(line);
        }
    }
    lines
}

fn pinned_natural_height(messages: &[ChatMessage], width: u16) -> u16 {
    (pinned_lines(messages, width).len() as u16).min(MAX_PINNED_HEIGHT)
}

fn draw_pinned_messages(frame: &mut Frame, area: Rect, messages: &[ChatMessage]) {
    if area.width == 0 || area.height == 0 || messages.is_empty() {
        return;
    }
    let mut lines = pinned_lines(messages, area.width);
    let max_rows = area.height as usize;
    if lines.len() > max_rows {
        lines.truncate(max_rows);
        if let Some(last) = lines.last_mut() {
            *last = Line::from(Span::styled(
                "  …",
                Style::default()
                    .fg(theme::TEXT_FAINT())
                    .add_modifier(Modifier::ITALIC),
            ));
        }
    }
    frame.render_widget(Paragraph::new(lines), area);
}

fn draw_amber_rule(frame: &mut Frame, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "─".repeat(area.width as usize),
            Style::default().fg(theme::AMBER_DIM()),
        ))),
        area,
    );
}

#[cfg(test)]
#[path = "ui_test.rs"]
mod ui_test;

