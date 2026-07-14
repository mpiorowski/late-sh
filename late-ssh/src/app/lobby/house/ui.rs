//! Full-screen house table: the game on top, a rule, and the table's
//! permanent chat below — the same vertical split the active room used.
//! The game sizes itself via `preferred_game_height`; chat absorbs the
//! rest. All four game renderers draw their own frames and key bars.

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::app::files::terminal_image::TerminalImageFrame;
use crate::app::{
    chat::ui::EmbeddedRoomChatView, common::theme, lobby::house::state::HouseTableClient,
};
use crate::usernames::UsernameLookup;

pub(crate) fn draw(
    frame: &mut Frame,
    area: Rect,
    client: Option<&HouseTableClient>,
    usernames: &UsernameLookup<'_>,
    chat: Option<EmbeddedRoomChatView<'_>>,
    terminal_images: &mut TerminalImageFrame,
) {
    let Some(client) = client else {
        frame.render_widget(
            Paragraph::new("The table is closed. Press Esc to head back to the Lobby.")
                .style(Style::default().fg(theme::TEXT_DIM())),
            area,
        );
        return;
    };

    let game_area = game_area(client, area);
    let spacer_height = if area.height > game_area.height { 1 } else { 0 };
    let chat_height = area
        .height
        .saturating_sub(game_area.height)
        .saturating_sub(spacer_height);
    let layout = Layout::vertical([
        Constraint::Length(game_area.height),
        Constraint::Length(spacer_height),
        Constraint::Length(chat_height),
    ])
    .split(area);

    client.draw(frame, layout[0], usernames);
    draw_spacer(frame, layout[1]);
    if let Some(chat) = chat {
        crate::app::chat::ui::draw_embedded_room_chat(frame, layout[2], chat, terminal_images);
    }
}

pub(crate) fn game_area(client: &HouseTableClient, area: Rect) -> Rect {
    let game_height = client.preferred_game_height(area).min(area.height).max(1);
    Rect {
        x: area.x,
        y: area.y,
        width: area.width,
        height: game_height,
    }
}

fn draw_spacer(frame: &mut Frame, area: Rect) {
    if area.height == 0 {
        return;
    }
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "─".repeat(area.width as usize),
            Style::default().fg(theme::BORDER()),
        ))),
        area,
    );
}
