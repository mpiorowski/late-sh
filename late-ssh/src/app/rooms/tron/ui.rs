use std::collections::HashMap;

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use uuid::Uuid;

use crate::app::{
    common::theme,
    rooms::tron::{
        state::{BOARD_HEIGHT, BOARD_WIDTH, Position, SEAT_COUNT, State, TronColor, TronOutcome},
        svc::TronSnapshot,
    },
};

const SIDE_WIDE: u16 = 30;
const SIDE_NARROW: u16 = 24;

pub fn draw_game(frame: &mut Frame, area: Rect, state: &State, usernames: &HashMap<Uuid, String>) {
    if area.height < 8 || area.width < 30 {
        draw_compact(frame, area, state);
        return;
    }

    if area.width >= 76 && area.height >= 14 {
        let side_w = if area.width >= 88 {
            SIDE_WIDE
        } else {
            SIDE_NARROW
        };
        let columns =
            Layout::horizontal([Constraint::Min(42), Constraint::Length(side_w)]).split(area);
        draw_table(frame, columns[0], state);
        draw_side(frame, columns[1], state, usernames);
    } else {
        draw_table(frame, area, state);
    }
}

fn draw_compact(frame: &mut Frame, area: Rect, state: &State) {
    let snapshot = state.snapshot();
    let lines = vec![
        Line::from(Span::styled(
            status_text(snapshot),
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        ))
        .alignment(Alignment::Center),
        Line::from(format!(
            "{}/4 seated · {}",
            snapshot.seats.iter().filter(|seat| seat.is_some()).count(),
            snapshot.speed_label
        ))
        .alignment(Alignment::Center),
    ];
    frame.render_widget(Paragraph::new(lines), area);
}

fn draw_table(frame: &mut Frame, area: Rect, state: &State) {
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(BOARD_HEIGHT as u16),
        Constraint::Length(1),
    ])
    .split(area);
    frame.render_widget(
        Paragraph::new(status_text(state.snapshot())).alignment(Alignment::Center),
        rows[0],
    );
    draw_board(frame, rows[1], state.snapshot());
    frame.render_widget(
        Paragraph::new(key_line(state)).alignment(Alignment::Center),
        rows[2],
    );
}

fn draw_board(frame: &mut Frame, area: Rect, snapshot: &TronSnapshot) {
    if area.height < BOARD_HEIGHT as u16 || area.width < BOARD_WIDTH as u16 {
        frame.render_widget(
            Paragraph::new("Grid needs more room.").alignment(Alignment::Center),
            area,
        );
        return;
    }

    let cell_width = if area.width >= (BOARD_WIDTH as u16) * 2 {
        2
    } else {
        1
    };
    let board_width = (BOARD_WIDTH as u16) * cell_width;
    let board_area = Rect {
        x: area.x + area.width.saturating_sub(board_width) / 2,
        y: area.y + area.height.saturating_sub(BOARD_HEIGHT as u16) / 2,
        width: board_width,
        height: BOARD_HEIGHT as u16,
    };

    let mut lines = Vec::with_capacity(BOARD_HEIGHT);
    for y in 0..BOARD_HEIGHT {
        let mut spans = Vec::with_capacity(BOARD_WIDTH);
        for x in 0..BOARD_WIDTH {
            let pos = Position {
                x: x as u8,
                y: y as u8,
            };
            spans.push(cell_span(snapshot, pos, cell_width));
        }
        lines.push(Line::from(spans));
    }
    frame.render_widget(Paragraph::new(lines), board_area);
}

fn cell_span(snapshot: &TronSnapshot, pos: Position, width: u16) -> Span<'static> {
    let index = pos.index();
    let head = head_at(snapshot, pos);
    let owner = snapshot.board[index].or(head);
    let text = if let Some(seat_index) = head {
        if snapshot.players[seat_index].crashed {
            "x"
        } else {
            "@"
        }
    } else if owner.is_some() {
        "#"
    } else {
        " "
    };
    let text = if width >= 2 {
        format!("{text}{text}")
    } else {
        text.to_string()
    };
    let style = owner
        .map(seat_style)
        .unwrap_or_else(|| Style::default().fg(theme::TEXT_FAINT()));
    Span::styled(text, style)
}

fn head_at(snapshot: &TronSnapshot, pos: Position) -> Option<usize> {
    snapshot
        .players
        .iter()
        .enumerate()
        .find_map(|(index, player)| {
            (player.head == Some(pos) && (player.alive || player.crashed)).then_some(index)
        })
}

fn draw_side(frame: &mut Frame, area: Rect, state: &State, usernames: &HashMap<Uuid, String>) {
    let snapshot = state.snapshot();
    let mut lines = vec![
        Line::from(Span::styled(
            status_text(snapshot),
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        )),
        Line::raw(""),
    ];
    for seat_index in 0..SEAT_COUNT {
        lines.push(player_line(seat_index, state, usernames));
    }
    lines.extend([
        Line::raw(""),
        info_line("speed", &snapshot.speed_label),
        info_line("alive", &format!("{}", alive_count(snapshot))),
        Line::raw(""),
    ]);
    if state.seat_index().is_some() {
        lines.extend([
            hint_line("arrows", "steer"),
            hint_line("w a s d", "steer"),
            hint_line("n", "start round"),
            hint_line("l", "leave seat"),
            hint_line("q", "leave room"),
        ]);
    } else {
        lines.extend([
            hint_line("s/Space/Enter", "sit"),
            hint_line("q", "leave room"),
        ]);
    }
    frame.render_widget(Paragraph::new(lines), area);
}

fn player_line(
    seat_index: usize,
    state: &State,
    usernames: &HashMap<Uuid, String>,
) -> Line<'static> {
    let snapshot = state.snapshot();
    let color = TronColor::for_seat(seat_index);
    let user_id = snapshot.seats[seat_index];
    let is_self = user_id.is_some_and(|uid| state.is_self(uid));
    let name = match user_id {
        Some(uid) => usernames
            .get(&uid)
            .cloned()
            .unwrap_or_else(|| "player".to_string()),
        None => "open".to_string(),
    };
    let marker = if is_self { "> " } else { "" };
    let state_text = if snapshot.players[seat_index].alive {
        "alive"
    } else if snapshot.players[seat_index].crashed {
        "crashed"
    } else {
        ""
    };
    Line::from(vec![
        Span::styled(format!("{} ", color.label()), seat_style(seat_index)),
        Span::styled(
            format!("{marker}{name}"),
            player_name_style(user_id, is_self),
        ),
        Span::styled(
            if state_text.is_empty() {
                String::new()
            } else {
                format!(" · {state_text}")
            },
            Style::default().fg(theme::TEXT_DIM()),
        ),
    ])
}

fn key_line(state: &State) -> Line<'static> {
    let seated = state.seat_index().is_some();
    let mut spans = Vec::new();
    if seated {
        spans.push(Span::styled(
            "arrows/wasd",
            Style::default().fg(theme::AMBER_DIM()),
        ));
        spans.push(Span::styled(
            " steer  ",
            Style::default().fg(theme::TEXT_DIM()),
        ));
        spans.push(Span::styled("n", Style::default().fg(theme::AMBER_DIM())));
        spans.push(Span::styled(
            " start  ",
            Style::default().fg(theme::TEXT_DIM()),
        ));
        spans.push(Span::styled("l", Style::default().fg(theme::AMBER_DIM())));
        spans.push(Span::styled(
            " seat  ",
            Style::default().fg(theme::TEXT_DIM()),
        ));
    } else {
        spans.push(Span::styled(
            "s/Space/Enter",
            Style::default().fg(theme::AMBER_DIM()),
        ));
        spans.push(Span::styled(
            " sit  ",
            Style::default().fg(theme::TEXT_DIM()),
        ));
    }
    spans.push(Span::styled("q", Style::default().fg(theme::AMBER_DIM())));
    spans.push(Span::styled(
        " room",
        Style::default().fg(theme::TEXT_DIM()),
    ));
    Line::from(spans)
}

fn status_text(snapshot: &TronSnapshot) -> String {
    match snapshot.outcome {
        Some(TronOutcome::Winner { seat_index }) => {
            format!("{} wins", TronColor::for_seat(seat_index).label())
        }
        Some(TronOutcome::Draw) => "Draw".to_string(),
        None => snapshot.status_message.clone(),
    }
}

fn alive_count(snapshot: &TronSnapshot) -> usize {
    snapshot
        .players
        .iter()
        .filter(|player| player.alive)
        .count()
}

fn info_line(label: &'static str, value: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("{label:<7}"),
            Style::default().fg(theme::TEXT_DIM()),
        ),
        Span::styled(value.to_string(), Style::default().fg(theme::TEXT_BRIGHT())),
    ])
}

fn hint_line(key: &str, label: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(key.to_string(), Style::default().fg(theme::AMBER_DIM())),
        Span::styled(format!("  {label}"), Style::default().fg(theme::TEXT_DIM())),
    ])
}

fn player_name_style(user_id: Option<Uuid>, is_self: bool) -> Style {
    if is_self {
        Style::default()
            .fg(theme::SUCCESS())
            .add_modifier(Modifier::BOLD)
    } else if user_id.is_some() {
        Style::default().fg(theme::TEXT())
    } else {
        Style::default().fg(theme::TEXT_DIM())
    }
}

fn seat_style(seat_index: usize) -> Style {
    match TronColor::for_seat(seat_index) {
        TronColor::Blue => Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
        TronColor::Pink => Style::default()
            .fg(Color::Magenta)
            .add_modifier(Modifier::BOLD),
        TronColor::Gold => Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD),
        TronColor::Green => Style::default()
            .fg(theme::SUCCESS())
            .add_modifier(Modifier::BOLD),
    }
}
