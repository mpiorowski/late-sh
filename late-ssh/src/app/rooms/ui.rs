use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::app::{
    common::theme,
    rooms::svc::{RoomsSnapshot, game_kind_label},
};

pub fn draw_rooms_page(
    frame: &mut Frame,
    area: Rect,
    add_form_open: bool,
    display_name: &str,
    snapshot: &RoomsSnapshot,
) {
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

    let layout = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(3),
        Constraint::Length(1),
        Constraint::Length(if add_form_open { 5 } else { 0 }),
        Constraint::Min(3),
    ])
    .split(inner);

    draw_add_button(frame, layout[1], add_form_open);

    if add_form_open {
        draw_display_name_input(frame, layout[3], display_name);
    }

    draw_room_list(frame, layout[4], snapshot);
}

fn draw_add_button(frame: &mut Frame, area: Rect, active: bool) {
    let style = if active {
        Style::default()
            .fg(theme::BG_SELECTION())
            .bg(theme::AMBER())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(theme::TEXT_BRIGHT())
            .add_modifier(Modifier::BOLD)
    };
    let border = if active {
        theme::BORDER_ACTIVE()
    } else {
        theme::BORDER()
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border));
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled("Add Blackjack Table", style)))
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

fn draw_room_list(frame: &mut Frame, area: Rect, snapshot: &RoomsSnapshot) {
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
        .map(|room| {
            Line::from(vec![
                Span::styled(&room.display_name, Style::default().fg(theme::TEXT())),
                Span::raw("  "),
                Span::styled(
                    game_kind_label(room.game_kind),
                    Style::default().fg(theme::AMBER()),
                ),
                Span::raw("  "),
                Span::styled(&room.status, Style::default().fg(theme::TEXT_DIM())),
            ])
        })
        .collect::<Vec<_>>();
    frame.render_widget(Paragraph::new(lines), inner);
}
