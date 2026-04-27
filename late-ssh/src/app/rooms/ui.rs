use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::app::{
    common::theme,
    rooms::{
        blackjack::state::State as BlackjackState,
        svc::{RoomListItem, RoomsSnapshot, game_kind_label},
    },
};

pub struct RoomsPageView<'a> {
    pub add_form_open: bool,
    pub display_name: &'a str,
    pub snapshot: &'a RoomsSnapshot,
    pub selected_index: usize,
    pub active_room: Option<&'a RoomListItem>,
    pub blackjack_state: &'a BlackjackState,
    pub is_admin: bool,
    pub is_mod: bool,
}

pub fn draw_rooms_page(frame: &mut Frame, area: Rect, view: &RoomsPageView<'_>) {
    let block = Block::default()
        .title(" Rooms ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 8 || inner.width < 36 {
        frame.render_widget(Paragraph::new("Terminal too small for Rooms"), inner);
        return;
    }

    if let Some(room) = view.active_room {
        draw_active_room(frame, inner, room, view.blackjack_state);
        return;
    }

    let layout = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(3),
        Constraint::Length(1),
        Constraint::Length(if view.add_form_open { 5 } else { 0 }),
        Constraint::Min(3),
    ])
    .split(inner);

    draw_add_button(
        frame,
        layout[1],
        view.add_form_open || view.selected_index == 0,
        view.is_admin,
    );

    if view.add_form_open {
        draw_display_name_input(frame, layout[3], view.display_name);
    }

    draw_room_list(
        frame,
        layout[4],
        view.snapshot,
        view.selected_index,
        view.is_admin || view.is_mod,
    );
}

fn draw_add_button(frame: &mut Frame, area: Rect, active: bool, enabled: bool) {
    let style = if !enabled {
        Style::default().fg(theme::TEXT_DIM())
    } else if active {
        Style::default()
            .fg(theme::BG_SELECTION())
            .bg(theme::AMBER())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(theme::TEXT_BRIGHT())
            .add_modifier(Modifier::BOLD)
    };
    let border = if active && enabled {
        theme::BORDER_ACTIVE()
    } else {
        theme::BORDER()
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border));
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            if enabled {
                "Add Blackjack Table"
            } else {
                "Add Blackjack Table (admin only)"
            },
            style,
        )))
        .block(block)
        .alignment(Alignment::Center),
        area,
    );
}

fn draw_display_name_input(frame: &mut Frame, area: Rect, display_name: &str) {
    let block = Block::default()
        .title(" Display Name ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let input_line = Line::from(vec![
        Span::styled(display_name.to_string(), Style::default().fg(theme::TEXT())),
        Span::styled("█", Style::default().fg(theme::AMBER())),
    ]);

    frame.render_widget(Paragraph::new(input_line), inner);
}

fn draw_room_list(
    frame: &mut Frame,
    area: Rect,
    snapshot: &RoomsSnapshot,
    selected_index: usize,
    can_enter: bool,
) {
    if area.height == 0 {
        return;
    }

    let block = Block::default()
        .title(" Tables ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if snapshot.rooms.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "No tables yet.",
                Style::default().fg(theme::TEXT_MUTED()),
            ))),
            inner,
        );
        return;
    }

    let lines = snapshot
        .rooms
        .iter()
        .take(inner.height as usize)
        .enumerate()
        .map(|(index, room)| {
            let selected = selected_index == index + 1;
            let name_style = if selected && can_enter {
                Style::default()
                    .fg(theme::BG_SELECTION())
                    .bg(theme::AMBER())
                    .add_modifier(Modifier::BOLD)
            } else if selected {
                Style::default()
                    .fg(theme::TEXT_BRIGHT())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme::TEXT())
            };
            let status = if can_enter {
                &room.status
            } else {
                "admin/mod only"
            };
            Line::from(vec![
                Span::styled(if selected { "> " } else { "  " }, name_style),
                Span::styled(&room.display_name, name_style),
                Span::raw("  "),
                Span::styled(
                    game_kind_label(room.game_kind),
                    Style::default().fg(theme::AMBER()),
                ),
                Span::raw("  "),
                Span::styled(status, Style::default().fg(theme::TEXT_DIM())),
            ])
        })
        .collect::<Vec<_>>();
    frame.render_widget(Paragraph::new(lines), inner);
}

fn draw_active_room(
    frame: &mut Frame,
    area: Rect,
    room: &RoomListItem,
    blackjack_state: &BlackjackState,
) {
    let layout = Layout::vertical([
        Constraint::Percentage(50),
        Constraint::Length(1),
        Constraint::Percentage(50),
    ])
    .split(area);

    draw_game_area(frame, layout[0], room, blackjack_state);
    draw_chat_placeholder(frame, layout[2], room);
}

fn draw_game_area(
    frame: &mut Frame,
    area: Rect,
    room: &RoomListItem,
    blackjack_state: &BlackjackState,
) {
    match room.game_kind {
        crate::app::rooms::svc::GameKind::Blackjack => {
            crate::app::rooms::blackjack::ui::draw_game(frame, area, blackjack_state, false);
        }
    }
}

fn draw_chat_placeholder(frame: &mut Frame, area: Rect, room: &RoomListItem) {
    let block = Block::default()
        .title(" Chat ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let lines = vec![
        Line::from(Span::styled(
            "Room chat will render here.",
            Style::default().fg(theme::TEXT_MUTED()),
        )),
        Line::from(Span::styled(
            room.chat_room_id.to_string(),
            Style::default().fg(theme::TEXT_DIM()),
        )),
    ];
    frame.render_widget(Paragraph::new(lines), inner);
}
