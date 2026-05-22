use std::collections::HashMap;

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};
use uuid::Uuid;

use crate::app::{
    common::theme,
    rooms::{
        game_ui::{
            draw_game_frame_with_info_sidebar, draw_game_overlay, info_label_value, info_tagline,
            key_hint,
        },
        tron::{
            state::{
                BOARD_HEIGHT, BOARD_WIDTH, Direction, Position, SEAT_COUNT, State, TronColor,
                TronOutcome, TronPhase,
            },
            svc::{TRON_FOUR_PLAYER_WIN_CHIPS, TRON_TWO_PLAYER_WIN_CHIPS, TronSnapshot},
        },
    },
};

// ── Layout ─────────────────────────────────────────────────────
// The grid is a fixed 56x28. With a one-cell border that is 58 wide;
// at two columns per cell it is 114 wide. The Info rail is the shared
// 28-column room sidebar. We hand the widest grid that fits, and
// only add the rail once both it and a full grid have room.

const SIDEBAR_WIDTH: u16 = 28;
const BORDERED_NARROW: u16 = BOARD_WIDTH as u16 + 2;
const BORDERED_WIDE: u16 = BOARD_WIDTH as u16 * 2 + 2;

// ── Arena palette ──────────────────────────────────────────────
// Dark grid, neon light-trails. The head is the bright tint and the
// trail wall a step dimmer, so the leading cell always reads first.
const ARENA_BG: Color = Color::Rgb(13, 15, 22);
const GRID_DOT: Color = Color::Rgb(34, 38, 48);

/// Height the grid wants: status row plus the bordered 20-tall arena.
pub fn preferred_height(area: Rect) -> u16 {
    (BOARD_HEIGHT as u16 + 3).min(area.height.max(1))
}

/// Resolve `(show_sidebar, cell_width)` for the available pane width.
fn plan(width: u16) -> (bool, u16) {
    if width >= BORDERED_WIDE + SIDEBAR_WIDTH {
        (true, 2)
    } else if width >= BORDERED_WIDE {
        (false, 2)
    } else if width >= BORDERED_NARROW + SIDEBAR_WIDTH {
        (true, 1)
    } else {
        (false, 1)
    }
}

// ── Entry point ────────────────────────────────────────────────

pub fn draw_game(frame: &mut Frame, area: Rect, state: &State, usernames: &HashMap<Uuid, String>) {
    if area.height < 8 || area.width < 30 {
        draw_compact(frame, area, state);
        return;
    }

    let (show_sidebar, cell_w) = plan(area.width);
    let info = info_lines(state, usernames);
    let content = draw_game_frame_with_info_sidebar(frame, area, "Tron", info, show_sidebar);

    let rows = if show_sidebar {
        Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).split(content)
    } else {
        Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(content)
    };

    frame.render_widget(
        Paragraph::new(status_line(state.snapshot())).alignment(Alignment::Center),
        rows[0],
    );
    draw_arena(frame, rows[1], state, cell_w);
    if !show_sidebar {
        frame.render_widget(
            Paragraph::new(key_line(state)).alignment(Alignment::Center),
            rows[2],
        );
    }
}

fn draw_compact(frame: &mut Frame, area: Rect, state: &State) {
    let snapshot = state.snapshot();
    let seated = snapshot.seats.iter().filter(|seat| seat.is_some()).count();
    let lines = vec![
        Line::from(Span::styled(
            status_text(snapshot),
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        ))
        .alignment(Alignment::Center),
        Line::from(format!("{seated}/4 seated · {}", snapshot.speed_label))
            .alignment(Alignment::Center),
    ];
    frame.render_widget(Paragraph::new(lines), area);
}

// ── Arena ──────────────────────────────────────────────────────

fn draw_arena(frame: &mut Frame, area: Rect, state: &State, cell_w: u16) {
    let snapshot = state.snapshot();
    let outer_h = BOARD_HEIGHT as u16 + 2;

    // Drop to one-wide cells if the chosen width would overflow.
    let mut cell_w = cell_w;
    let mut outer_w = BOARD_WIDTH as u16 * cell_w + 2;
    if outer_w > area.width {
        cell_w = 1;
        outer_w = BORDERED_NARROW;
    }

    if area.width < outer_w || area.height < outer_h {
        frame.render_widget(
            Paragraph::new("Grid needs more room.").alignment(Alignment::Center),
            area,
        );
        return;
    }

    let arena = Rect {
        x: area.x + (area.width - outer_w) / 2,
        y: area.y + (area.height - outer_h) / 2,
        width: outer_w,
        height: outer_h,
    };

    let border_color = match snapshot.phase {
        TronPhase::Running => theme::AMBER(),
        TronPhase::Finished => theme::SUCCESS(),
        TronPhase::Waiting => theme::BORDER(),
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(arena_title(state))
        .title_alignment(Alignment::Center)
        .style(Style::default().bg(ARENA_BG));
    let inner = block.inner(arena);
    frame.render_widget(block, arena);
    frame.render_widget(Paragraph::new(board_lines(snapshot, cell_w)), inner);

    if snapshot.phase == TronPhase::Finished {
        let (heading, subtitle, color) = outcome_overlay(snapshot);
        draw_game_overlay(frame, inner, heading, &subtitle, color);
    }
}

fn arena_title(state: &State) -> Line<'static> {
    match state.user_color() {
        Some(color) => Line::from(vec![
            Span::styled(" you ride ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled(
                color.label(),
                Style::default()
                    .fg(head_color_of(color))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
        ]),
        None => Line::from(Span::styled(
            " spectating ",
            Style::default().fg(theme::TEXT_DIM()),
        )),
    }
}

fn board_lines(snapshot: &TronSnapshot, cell_w: u16) -> Vec<Line<'static>> {
    let mut lines = Vec::with_capacity(BOARD_HEIGHT);
    for y in 0..BOARD_HEIGHT {
        let mut spans = Vec::with_capacity(BOARD_WIDTH);
        for x in 0..BOARD_WIDTH {
            let pos = Position {
                x: x as u8,
                y: y as u8,
            };
            spans.push(cell_span(snapshot, pos, cell_w));
        }
        lines.push(Line::from(spans));
    }
    lines
}

fn cell_span(snapshot: &TronSnapshot, pos: Position, cell_w: u16) -> Span<'static> {
    let width = cell_w as usize;

    // Heads sit on top of their own trail, so resolve them first.
    if let Some(seat) = head_at(snapshot, pos) {
        let player = snapshot.players[seat];
        return if player.crashed {
            Span::styled(
                pad_glyph('x', width),
                Style::default()
                    .bg(trail_color(seat))
                    .fg(ARENA_BG)
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            Span::styled(
                pad_glyph(direction_glyph(player.direction), width),
                Style::default()
                    .bg(head_color(seat))
                    .fg(ARENA_BG)
                    .add_modifier(Modifier::BOLD),
            )
        };
    }

    if let Some(seat) = snapshot.board[pos.index()] {
        return Span::styled(
            "█".repeat(width),
            Style::default().fg(trail_color(seat)).bg(ARENA_BG),
        );
    }

    // Empty cell: a faint checkered dot keeps the grid legible.
    let dotted = (pos.x as usize + pos.y as usize).is_multiple_of(2);
    let text = if dotted {
        format!("·{}", " ".repeat(width - 1))
    } else {
        " ".repeat(width)
    };
    Span::styled(text, Style::default().fg(GRID_DOT).bg(ARENA_BG))
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

fn pad_glyph(glyph: char, width: usize) -> String {
    let mut text = String::with_capacity(width);
    text.push(glyph);
    for _ in 1..width {
        text.push(' ');
    }
    text
}

fn direction_glyph(direction: Direction) -> char {
    match direction {
        Direction::Up => '▲',
        Direction::Down => '▼',
        Direction::Left => '◀',
        Direction::Right => '▶',
    }
}

// ── Info sidebar ───────────────────────────────────────────────

fn info_lines(state: &State, usernames: &HashMap<Uuid, String>) -> Vec<Line<'static>> {
    let snapshot = state.snapshot();
    let mut lines = vec![
        info_tagline("Light-cycle grid."),
        info_tagline("Last rider home wins."),
        Line::raw(""),
        section_header("Riders"),
    ];
    for seat in 0..SEAT_COUNT {
        lines.push(rider_line(seat, state, usernames));
    }
    lines.extend([
        Line::raw(""),
        info_label_value("Speed", snapshot.speed_label.clone(), theme::AMBER()),
        info_label_value(
            "Alive",
            alive_count(snapshot).to_string(),
            theme::TEXT_BRIGHT(),
        ),
        info_label_value(
            "Prize",
            format!("{TRON_TWO_PLAYER_WIN_CHIPS}-{TRON_FOUR_PLAYER_WIN_CHIPS}"),
            theme::SUCCESS(),
        ),
        info_label_value("State", state_label(snapshot), theme::SUCCESS()),
        Line::raw(""),
        section_header("Controls"),
    ]);
    if state.seat_index().is_some() {
        lines.extend([
            key_hint("arrows/wasd", "steer"),
            key_hint("n", "start round"),
            key_hint("l", "leave seat"),
            key_hint("q", "leave room"),
        ]);
    } else {
        lines.extend([
            key_hint("s/space", "take a seat"),
            key_hint("q", "leave room"),
        ]);
    }
    lines
}

fn rider_line(seat: usize, state: &State, usernames: &HashMap<Uuid, String>) -> Line<'static> {
    let snapshot = state.snapshot();
    let color = TronColor::for_seat(seat);
    let user = snapshot.seats[seat];
    let is_self = user.is_some_and(|uid| state.is_self(uid));
    let name = match user {
        Some(uid) => usernames
            .get(&uid)
            .cloned()
            .unwrap_or_else(|| "rider".to_string()),
        None => "open".to_string(),
    };
    let player = snapshot.players[seat];
    let status = if player.alive {
        "alive"
    } else if player.crashed {
        "crashed"
    } else {
        ""
    };

    let mut spans = vec![
        Span::styled(
            if is_self { "> " } else { "  " },
            Style::default().fg(theme::AMBER()),
        ),
        Span::styled(
            format!("{:<6}", color.label()),
            Style::default()
                .fg(head_color_of(color))
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(name, rider_name_style(user, is_self)),
    ];
    if !status.is_empty() {
        spans.push(Span::styled(
            format!("  {status}"),
            Style::default().fg(theme::TEXT_DIM()),
        ));
    }
    Line::from(spans)
}

fn rider_name_style(user: Option<Uuid>, is_self: bool) -> Style {
    if is_self {
        Style::default()
            .fg(theme::SUCCESS())
            .add_modifier(Modifier::BOLD)
    } else if user.is_some() {
        Style::default().fg(theme::TEXT())
    } else {
        Style::default().fg(theme::TEXT_FAINT())
    }
}

fn section_header(text: &str) -> Line<'static> {
    Line::from(Span::styled(
        text.to_string(),
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD),
    ))
}

// ── Status / keys / overlay ────────────────────────────────────

fn status_line(snapshot: &TronSnapshot) -> Line<'static> {
    let color = match snapshot.phase {
        TronPhase::Running => theme::AMBER(),
        TronPhase::Finished => theme::SUCCESS(),
        TronPhase::Waiting => theme::TEXT_DIM(),
    };
    Line::from(Span::styled(
        status_text(snapshot),
        Style::default().fg(color).add_modifier(Modifier::BOLD),
    ))
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

fn state_label(snapshot: &TronSnapshot) -> String {
    match snapshot.outcome {
        Some(TronOutcome::Winner { seat_index }) => {
            format!("{} won", TronColor::for_seat(seat_index).label())
        }
        Some(TronOutcome::Draw) => "draw".to_string(),
        None => match snapshot.phase {
            TronPhase::Running => "running".to_string(),
            TronPhase::Waiting => "waiting".to_string(),
            TronPhase::Finished => "finished".to_string(),
        },
    }
}

fn outcome_overlay(snapshot: &TronSnapshot) -> (&'static str, String, Color) {
    match snapshot.outcome {
        Some(TronOutcome::Winner { seat_index }) => (
            "Winner",
            format!("{} wins · press n", TronColor::for_seat(seat_index).label()),
            theme::SUCCESS(),
        ),
        Some(TronOutcome::Draw) => (
            "Draw",
            "grid locked · press n".to_string(),
            theme::TEXT_MUTED(),
        ),
        None => (
            "Round over",
            "press n to ride again".to_string(),
            theme::AMBER(),
        ),
    }
}

fn key_line(state: &State) -> Line<'static> {
    let seated = state.seat_index().is_some();
    let hint = |spans: &mut Vec<Span<'static>>, key: &str, desc: &str| {
        spans.push(Span::styled(
            key.to_string(),
            Style::default().fg(theme::AMBER()),
        ));
        spans.push(Span::styled(
            format!(" {desc}   "),
            Style::default().fg(theme::TEXT_DIM()),
        ));
    };

    let mut spans = Vec::new();
    if seated {
        hint(&mut spans, "arrows/wasd", "steer");
        hint(&mut spans, "n", "start");
        hint(&mut spans, "l", "leave seat");
    } else {
        hint(&mut spans, "s/space", "take a seat");
    }
    hint(&mut spans, "q", "leave room");

    // Trim the trailing separator padding from the final hint.
    if let Some(last) = spans.last_mut() {
        let trimmed = last.content.trim_end().to_string();
        *last = Span::styled(trimmed, Style::default().fg(theme::TEXT_DIM()));
    }
    Line::from(spans)
}

fn alive_count(snapshot: &TronSnapshot) -> usize {
    snapshot
        .players
        .iter()
        .filter(|player| player.alive)
        .count()
}

// ── Seat colours ───────────────────────────────────────────────

fn head_color(seat: usize) -> Color {
    head_color_of(TronColor::for_seat(seat))
}

fn head_color_of(color: TronColor) -> Color {
    match color {
        TronColor::Blue => Color::Rgb(96, 206, 255),
        TronColor::Pink => Color::Rgb(255, 108, 198),
        TronColor::Gold => Color::Rgb(255, 200, 84),
        TronColor::Green => Color::Rgb(112, 232, 138),
    }
}

fn trail_color(seat: usize) -> Color {
    match TronColor::for_seat(seat) {
        TronColor::Blue => Color::Rgb(46, 116, 168),
        TronColor::Pink => Color::Rgb(166, 64, 130),
        TronColor::Gold => Color::Rgb(170, 130, 50),
        TronColor::Green => Color::Rgb(62, 146, 84),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::rooms::tron::{state::BOARD_CELLS, svc::TronPlayerSnapshot};

    fn blank_snapshot() -> TronSnapshot {
        TronSnapshot {
            room_id: Uuid::nil(),
            seats: [None; SEAT_COUNT],
            board: [None; BOARD_CELLS],
            players: [TronPlayerSnapshot {
                head: None,
                direction: Direction::Right,
                alive: false,
                crashed: false,
            }; SEAT_COUNT],
            phase: TronPhase::Waiting,
            outcome: None,
            status_message: "test".to_string(),
            speed_label: "standard".to_string(),
        }
    }

    #[test]
    fn board_lines_have_uniform_width() {
        let snapshot = blank_snapshot();
        for cell_w in [1u16, 2] {
            let lines = board_lines(&snapshot, cell_w);
            assert_eq!(lines.len(), BOARD_HEIGHT);
            for line in &lines {
                let width: usize = line
                    .spans
                    .iter()
                    .map(|span| span.content.chars().count())
                    .sum();
                assert_eq!(width, BOARD_WIDTH * cell_w as usize);
            }
        }
    }

    #[test]
    fn plan_prefers_widest_grid_that_fits() {
        assert_eq!(plan(40), (false, 1));
        assert_eq!(plan(70), (false, 1));
        assert_eq!(plan(86), (true, 1));
        assert_eq!(plan(114), (false, 2));
        assert_eq!(plan(142), (true, 2));
    }
}
