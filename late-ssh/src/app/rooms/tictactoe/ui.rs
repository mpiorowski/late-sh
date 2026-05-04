use std::collections::HashMap;

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};
use uuid::Uuid;

use crate::app::{
    common::theme,
    rooms::tictactoe::state::{State, Winner},
};

pub fn draw_game(frame: &mut Frame, area: Rect, state: &State, usernames: &HashMap<Uuid, String>) {
    let block = Block::default()
        .title(" Tic-Tac-Toe ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 9 || inner.width < 32 {
        draw_compact(frame, inner, state);
        return;
    }

    let columns = Layout::horizontal([Constraint::Min(19), Constraint::Length(28)]).split(inner);
    draw_board(frame, columns[0], state);
    draw_side(frame, columns[1], state, usernames);
}

fn draw_compact(frame: &mut Frame, area: Rect, state: &State) {
    let snapshot = state.snapshot();
    let mut lines = Vec::new();
    for row in 0..3 {
        let mut spans = Vec::new();
        for col in 0..3 {
            let index = row * 3 + col;
            let cell = snapshot.board[index]
                .map(|mark| mark.label())
                .unwrap_or("·");
            let selected = index == state.cursor();
            spans.push(Span::styled(
                format!(" {cell} "),
                cell_style(selected, snapshot.board[index].is_some()),
            ));
        }
        lines.push(Line::from(spans).alignment(Alignment::Center));
    }
    lines.push(Line::from(status_text(state)).alignment(Alignment::Center));
    frame.render_widget(Paragraph::new(lines), area);
}

fn draw_board(frame: &mut Frame, area: Rect, state: &State) {
    let snapshot = state.snapshot();
    let mut lines = Vec::new();
    for row in 0..3 {
        let mut spans = vec![Span::raw("  ")];
        for col in 0..3 {
            let index = row * 3 + col;
            let cell = snapshot.board[index]
                .map(|mark| mark.label())
                .unwrap_or(" ");
            let selected = index == state.cursor();
            spans.push(Span::styled(
                format!("  {cell}  "),
                cell_style(selected, snapshot.board[index].is_some()),
            ));
            if col < 2 {
                spans.push(Span::styled("│", Style::default().fg(theme::BORDER_DIM())));
            }
        }
        lines.push(Line::from(spans).alignment(Alignment::Center));
        if row < 2 {
            lines.push(
                Line::from(Span::styled(
                    "─────┼─────┼─────",
                    Style::default().fg(theme::BORDER_DIM()),
                ))
                .alignment(Alignment::Center),
            );
        }
    }
    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), area);
}

fn draw_side(frame: &mut Frame, area: Rect, state: &State, usernames: &HashMap<Uuid, String>) {
    let snapshot = state.snapshot();
    let lines = vec![
        Line::from(status_text(state)),
        Line::raw(""),
        player_line("X", snapshot.seats[0], usernames),
        player_line("O", snapshot.seats[1], usernames),
        Line::raw(""),
        Line::from(vec![
            Span::styled("1-9", Style::default().fg(theme::AMBER_DIM())),
            Span::styled(" place direct", Style::default().fg(theme::TEXT_DIM())),
        ]),
        Line::from(vec![
            Span::styled("Space/Enter", Style::default().fg(theme::AMBER_DIM())),
            Span::styled(" place cursor", Style::default().fg(theme::TEXT_DIM())),
        ]),
        Line::from(vec![
            Span::styled("w/a/d/x", Style::default().fg(theme::AMBER_DIM())),
            Span::styled(" move cursor", Style::default().fg(theme::TEXT_DIM())),
        ]),
        Line::from(vec![
            Span::styled("s", Style::default().fg(theme::AMBER_DIM())),
            Span::styled(" sit  ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled("l", Style::default().fg(theme::AMBER_DIM())),
            Span::styled(" leave", Style::default().fg(theme::TEXT_DIM())),
        ]),
        Line::from(vec![
            Span::styled("n", Style::default().fg(theme::AMBER_DIM())),
            Span::styled(" new round", Style::default().fg(theme::TEXT_DIM())),
        ]),
    ];
    frame.render_widget(Paragraph::new(lines), area);
}

fn player_line(
    mark: &'static str,
    user_id: Option<Uuid>,
    usernames: &HashMap<Uuid, String>,
) -> Line<'static> {
    let name = user_id
        .and_then(|user_id| usernames.get(&user_id).cloned())
        .unwrap_or_else(|| "open seat".to_string());
    Line::from(vec![
        Span::styled(format!("{mark} "), Style::default().fg(theme::AMBER())),
        Span::styled(name, Style::default().fg(theme::TEXT())),
    ])
}

fn status_text(state: &State) -> String {
    let snapshot = state.snapshot();
    match snapshot.winner {
        Some(Winner::Mark(mark)) => format!("{} wins", mark.label()),
        Some(Winner::Draw) => "Draw".to_string(),
        None => snapshot.status_message.clone(),
    }
}

fn cell_style(selected: bool, occupied: bool) -> Style {
    if selected {
        Style::default()
            .fg(theme::BG_SELECTION())
            .bg(theme::AMBER())
            .add_modifier(Modifier::BOLD)
    } else if occupied {
        Style::default()
            .fg(theme::TEXT_BRIGHT())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_DIM())
    }
}
