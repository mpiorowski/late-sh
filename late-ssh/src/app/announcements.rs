use anyhow::Result;
use chrono::{DateTime, Utc};
use late_core::models::chat_message::ChatMessage;
use late_core::models::chat_room::ChatRoom;
use late_core::models::chat_room_member::ChatRoomMember;
use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Margin, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};
use tokio_postgres::Client;
use uuid::Uuid;

use crate::app::common::{markdown::render_body_to_lines, primitives::format_relative_time, theme};

const ANNOUNCEMENTS_SLUG: &str = "announcements";
const LOGIN_ANNOUNCEMENT_LIMIT: i64 = 10;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoginAnnouncementMessage {
    pub id: Uuid,
    pub created: DateTime<Utc>,
    pub author: String,
    pub body: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoginAnnouncements {
    pub room_id: Uuid,
    pub messages: Vec<LoginAnnouncementMessage>,
    pub scroll_offset: u16,
}

impl LoginAnnouncements {
    pub fn len(&self) -> usize {
        self.messages.len()
    }

    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    pub fn scroll(&mut self, delta: i16) {
        if delta.is_negative() {
            self.scroll_offset = self.scroll_offset.saturating_sub(delta.unsigned_abs());
        } else {
            self.scroll_offset = self.scroll_offset.saturating_add(delta as u16);
        }
    }

    pub fn latest_displayed_at(&self) -> Option<DateTime<Utc>> {
        self.messages.iter().map(|message| message.created).max()
    }
}

pub async fn load_login_announcements(
    client: &Client,
    user_id: Uuid,
) -> Result<Option<LoginAnnouncements>> {
    let Some(room) = ChatRoom::find_public_non_dm_by_slug(client, ANNOUNCEMENTS_SLUG).await? else {
        return Ok(None);
    };
    let room_id = room.id;

    ChatRoomMember::join(client, room_id, user_id).await?;

    let messages: Vec<LoginAnnouncementMessage> =
        ChatMessage::list_unread_for_member(client, room_id, user_id, LOGIN_ANNOUNCEMENT_LIMIT)
            .await?
            .into_iter()
            .map(|message| LoginAnnouncementMessage {
                id: message.id,
                created: message.created,
                author: message.author,
                body: message.body,
            })
            .collect();

    if messages.is_empty() {
        return Ok(None);
    }

    Ok(Some(LoginAnnouncements {
        room_id,
        messages,
        scroll_offset: 0,
    }))
}

pub(crate) fn draw(frame: &mut Frame, area: Rect, announcements: &LoginAnnouncements) {
    let popup = centered_rect(78, 26, area);
    frame.render_widget(Clear, popup);

    let title = format!(" #announcements - {} new ", announcements.len());
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

    if inner.width < 24 || inner.height < 5 {
        return;
    }

    let layout = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(1),
    ])
    .split(inner);

    let body_area = layout[1].inner(Margin {
        horizontal: 2,
        vertical: 0,
    });
    let lines = announcement_lines(announcements, body_area.width);
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((announcements.scroll_offset, 0)),
        body_area,
    );

    let footer = Line::from(vec![
        Span::styled(" j/k", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" scroll  ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled(" Enter", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" continue  ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("Esc/q", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" close", Style::default().fg(theme::TEXT_DIM())),
    ]);
    frame.render_widget(Paragraph::new(footer).centered(), layout[2]);
}

fn announcement_lines(announcements: &LoginAnnouncements, width: u16) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let body_width = width.saturating_sub(2).max(1) as usize;
    for (index, message) in announcements.messages.iter().enumerate() {
        if index > 0 {
            lines.push(Line::from(""));
        }
        lines.push(Line::from(vec![
            Span::styled(
                format!("@{}", message.author),
                Style::default()
                    .fg(theme::TEXT_BRIGHT())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" {}", format_relative_time(message.created)),
                Style::default().fg(theme::TEXT_FAINT()),
            ),
        ]));
        lines.extend(render_body_to_lines(
            &message.body,
            body_width,
            Span::raw(" "),
            Style::default().fg(theme::TEXT()),
        ));
    }
    lines
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
    use super::LoginAnnouncements;

    #[test]
    fn scroll_is_not_capped_to_message_count() {
        let mut announcements = LoginAnnouncements {
            room_id: uuid::Uuid::nil(),
            messages: Vec::new(),
            scroll_offset: 0,
        };

        announcements.scroll(20);
        assert_eq!(announcements.scroll_offset, 20);

        announcements.scroll(-3);
        assert_eq!(announcements.scroll_offset, 17);

        announcements.scroll(-99);
        assert_eq!(announcements.scroll_offset, 0);
    }
}
