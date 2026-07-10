use std::collections::VecDeque;

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
    dashboard::state::DashboardRoomJoin,
    files::terminal_image::TerminalImageFrame,
    rooms::{
        registry::{RoomDirectorySummary, RoomGameRegistry},
        svc::{RoomListItem, RoomsSnapshot},
    },
};
use late_core::models::chat_message::ChatMessage;

#[derive(Clone, Debug)]
pub struct DashboardRoomCard {
    pub room: RoomListItem,
    pub game_label: &'static str,
    pub occupied_seats: Option<usize>,
    pub total_seats: usize,
    pub recent_join_user_id: Option<uuid::Uuid>,
}

impl DashboardRoomCard {
    fn new(room: &RoomListItem, summary: RoomDirectorySummary) -> Self {
        Self {
            room: room.clone(),
            game_label: summary.game_label,
            occupied_seats: summary.occupied_seats,
            total_seats: summary.total_seats,
            recent_join_user_id: None,
        }
    }

    fn with_recent_join_user(mut self, user_id: uuid::Uuid) -> Self {
        self.recent_join_user_id = Some(user_id);
        self
    }
}

pub(crate) fn recent_dashboard_rooms(
    snapshot: &RoomsSnapshot,
    registry: &RoomGameRegistry,
    recent_joins: &VecDeque<DashboardRoomJoin>,
    max: usize,
) -> Vec<DashboardRoomCard> {
    let mut rooms = Vec::new();
    for join in recent_joins {
        let Some(room) = snapshot.rooms.iter().find(|room| room.id == join.room_id) else {
            continue;
        };
        rooms.push(
            DashboardRoomCard::new(room, registry.directory_summary(room))
                .with_recent_join_user(join.user_id),
        );
        if rooms.len() >= max {
            break;
        }
    }
    rooms
}

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
mod tests {
    use super::*;
    use chrono::Utc;
    use late_core::models::chat_message::ChatMessage;
    use uuid::Uuid;

    const TEST_WIDTH: u16 = 80;

    fn pin(body: &str) -> ChatMessage {
        let now = Utc::now();
        ChatMessage {
            id: Uuid::nil(),
            created: now,
            updated: now,
            pinned: true,
            reply_to_message_id: None,
            reply_to_user_id: None,
            room_id: Uuid::nil(),
            user_id: Uuid::nil(),
            body: body.to_string(),
        }
    }

    #[test]
    fn dashboard_pinned_height_zero_without_pins() {
        assert_eq!(dashboard_pinned_height(40, TEST_WIDTH, &[]), 0);
    }

    #[test]
    fn dashboard_pinned_height_present_when_space_allows() {
        let pins = [pin("hello")];
        let height = dashboard_pinned_height(40, TEST_WIDTH, &pins);
        assert!(height > 0);
    }

    #[test]
    fn dashboard_pinned_height_yields_to_minimum_chat() {
        let pins = [pin("hello")];
        assert_eq!(
            dashboard_pinned_height(MIN_CHAT_HEIGHT_WITH_LOUNGE, TEST_WIDTH, &pins),
            0
        );
    }

    #[test]
    fn pinned_natural_height_wraps_and_sums() {
        let pins = [
            pin("short"),
            pin(&"word ".repeat(40)), // forces multi-line wrap at width 80
        ];
        let height = pinned_natural_height(&pins, TEST_WIDTH);
        assert!(height >= 2, "expected wrapping to add rows, got {height}");
        assert!(height <= MAX_PINNED_HEIGHT);
    }

    #[test]
    fn pinned_natural_height_caps_at_max() {
        let pins: Vec<ChatMessage> = (0..20).map(|i| pin(&format!("pin {i}"))).collect();
        let height = pinned_natural_height(&pins, TEST_WIDTH);
        assert_eq!(height, MAX_PINNED_HEIGHT);
    }
}
