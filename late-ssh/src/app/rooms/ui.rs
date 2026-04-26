use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::app::common::theme;

use super::data::{ROOMS, RoomCard};

pub fn draw_rooms_page(
    frame: &mut Frame,
    area: Rect,
    selection: usize,
    active_room: Option<usize>,
    is_admin: bool,
) {
    let block = Block::default()
        .title(" Rooms ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 10 || inner.width < 50 {
        frame.render_widget(Paragraph::new("Terminal too small for Rooms"), inner);
        return;
    }

    if let Some(room_idx) = active_room {
        draw_room_shell(
            frame,
            inner,
            &ROOMS[room_idx.min(ROOMS.len() - 1)],
            is_admin,
        );
        return;
    }

    let layout = Layout::vertical([
        Constraint::Length(4),
        Constraint::Length(1),
        Constraint::Min(0),
    ])
    .split(inner);

    let header = vec![
        Line::from(Span::styled(
            "Hardcoded room directory for multiplayer table games.",
            Style::default()
                .fg(theme::TEXT_MUTED())
                .add_modifier(Modifier::ITALIC),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("Keys ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled("j/k", Style::default().fg(theme::AMBER())),
            Span::styled(" move  ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled("Enter", Style::default().fg(theme::AMBER())),
            Span::styled(
                if is_admin {
                    " enter room"
                } else {
                    " unavailable for non-admins"
                },
                Style::default().fg(theme::TEXT_DIM()),
            ),
        ]),
    ];
    frame.render_widget(Paragraph::new(header), layout[0]);

    let rows = Layout::vertical([Constraint::Length(6), Constraint::Length(6)]).split(layout[2]);
    for (idx, room) in ROOMS.iter().enumerate() {
        draw_room_card(
            frame,
            rows[idx],
            room,
            idx == selection.min(ROOMS.len() - 1),
            is_admin,
        );
    }
}

fn draw_room_card(
    frame: &mut Frame,
    area: Rect,
    room: &RoomCard<'_>,
    selected: bool,
    is_admin: bool,
) {
    let border = if selected {
        theme::BORDER_ACTIVE()
    } else {
        theme::BORDER()
    };
    let title_style = if selected {
        Style::default()
            .fg(theme::TEXT_BRIGHT())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT())
    };
    let block = Block::default()
        .title(if selected {
            " Selected Room "
        } else {
            " Room "
        })
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let lines = vec![
        Line::from(vec![
            Span::styled("> ", title_style),
            Span::styled(room.title, title_style),
            Span::styled(
                format!("  [{}]", room.slug),
                Style::default().fg(theme::TEXT_DIM()),
            ),
        ]),
        Line::from(vec![
            Span::styled("Game: ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled(room.game, Style::default().fg(theme::TEXT())),
            Span::styled("   Status: ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled(
                if is_admin { room.status } else { "In Progress" },
                Style::default().fg(if is_admin {
                    theme::SUCCESS()
                } else {
                    theme::AMBER()
                }),
            ),
        ]),
        Line::from(vec![
            Span::styled("Seats: ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled(room.seats, Style::default().fg(theme::TEXT())),
        ]),
    ];
    frame.render_widget(Paragraph::new(lines), inner);
}

fn draw_room_shell(frame: &mut Frame, area: Rect, room: &RoomCard<'_>, is_admin: bool) {
    let layout = Layout::vertical([
        Constraint::Length(5),
        Constraint::Length(1),
        Constraint::Length(8),
        Constraint::Min(0),
    ])
    .split(area);

    let header = vec![
        Line::from(vec![
            Span::styled(
                room.title,
                Style::default()
                    .fg(theme::TEXT_BRIGHT())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  [{}]", room.slug),
                Style::default().fg(theme::TEXT_DIM()),
            ),
        ]),
        Line::from(Span::styled(
            "Room shell only. Shared table routing is not connected yet.",
            Style::default()
                .fg(theme::TEXT_MUTED())
                .add_modifier(Modifier::ITALIC),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("Keys ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled("Esc", Style::default().fg(theme::AMBER())),
            Span::styled(" back  ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled("Enter", Style::default().fg(theme::AMBER())),
            Span::styled(
                " placeholder action",
                Style::default().fg(theme::TEXT_DIM()),
            ),
        ]),
    ];
    frame.render_widget(Paragraph::new(header), layout[0]);

    let details_block = Block::default()
        .title(" Room State ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
    let details_inner = details_block.inner(layout[2]);
    frame.render_widget(details_block, layout[2]);

    let status = if is_admin { room.status } else { "In Progress" };
    let status_color = if is_admin {
        theme::SUCCESS()
    } else {
        theme::AMBER()
    };
    let lines = vec![
        Line::from(vec![
            Span::styled("Game: ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled(room.game, Style::default().fg(theme::TEXT())),
        ]),
        Line::from(vec![
            Span::styled("Status: ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled(status, Style::default().fg(status_color)),
        ]),
        Line::from(vec![
            Span::styled("Seats: ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled(room.seats, Style::default().fg(theme::TEXT())),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "Next wiring step: bind this room slug/id to room-scoped blackjack service state.",
            Style::default().fg(theme::TEXT_DIM()),
        )),
    ];
    frame.render_widget(Paragraph::new(lines), details_inner);
}
