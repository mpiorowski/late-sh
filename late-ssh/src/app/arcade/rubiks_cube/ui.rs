use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use super::state::{Face, State, Sticker, face_for_view, oriented_face, view_label};
use crate::app::arcade::ui::{
    GameBottomBar, centered_rect, draw_game_frame, draw_game_overlay, keys_line, status_line,
    tip_line,
};
use crate::app::common::theme;

pub fn draw_game(frame: &mut Frame, area: Rect, state: &State, show_bottom_bar: bool) {
    let bottom = GameBottomBar {
        status: status_line(vec![
            ("moves", state.move_count().to_string(), theme::AMBER_GLOW()),
            ("scramble", scramble_label(state), theme::SUCCESS()),
            (
                "view",
                view_label(state.view_turns()).to_string(),
                theme::TEXT_BRIGHT(),
            ),
        ]),
        keys: keys_line(vec![
            ("u/d/l/r/f/b", "turn"),
            ("Shift", "inverse"),
            ("s", "scramble"),
            ("v", "view"),
            ("z/y", "undo/redo"),
            ("0", "reset"),
            ("Esc", "exit"),
        ]),
        tip: Some(tip_line(state.message().to_string())),
    };

    let board_area = draw_game_frame(frame, area, "Rubik's Cube", bottom, show_bottom_bar);
    if board_area.width < 42 || board_area.height < 18 {
        frame.render_widget(
            Paragraph::new("Terminal too small for Rubik's Cube").alignment(Alignment::Center),
            board_area,
        );
        return;
    }

    let content = centered_rect(
        board_area,
        86.min(board_area.width),
        24.min(board_area.height),
    );
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(44), Constraint::Length(30)])
        .split(content);

    draw_cube(frame, columns[0], state);
    draw_net(frame, columns[1], state);

    if state.is_solved() && state.move_count() > 0 {
        draw_game_overlay(
            frame,
            board_area,
            "SOLVED",
            &format!("{} moves", state.move_count()),
            theme::SUCCESS(),
        );
    }
}

fn scramble_label(state: &State) -> String {
    if state.scramble_id() == 0 {
        "none".to_string()
    } else {
        format!("#{}", state.scramble_id())
    }
}

fn draw_cube(frame: &mut Frame, area: Rect, state: &State) {
    let (top_face, front_face, right_face) = face_for_view(state.view_turns());
    let top = oriented_face(state.stickers(), top_face, state.view_turns());
    let front = oriented_face(state.stickers(), front_face, state.view_turns());
    let right = oriented_face(state.stickers(), right_face, state.view_turns());

    let mut lines = Vec::new();
    lines.push(Line::from(Span::styled(
        format!(
            "Visible: {} top / {} front / {} right",
            top_face.label(),
            front_face.label(),
            right_face.label()
        ),
        Style::default().fg(theme::TEXT_DIM()),
    )));
    lines.push(Line::from(""));

    for row in 0..3 {
        let mut spans = Vec::new();
        spans.push(Span::raw(" ".repeat(12 - row * 2)));
        push_face_row(&mut spans, top[row], 4);
        lines.push(Line::from(spans));
    }

    for row in 0..3 {
        let mut spans = Vec::new();
        spans.push(Span::raw("      "));
        push_face_row(&mut spans, front[row], 4);
        spans.push(Span::raw(" ".repeat(2 + row * 2)));
        push_face_row(&mut spans, right[row], 4);
        lines.push(Line::from(spans));
    }

    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Left), area);
}

fn draw_net(frame: &mut Frame, area: Rect, state: &State) {
    let stickers = state.stickers();
    let mut lines = vec![Line::from(Span::styled(
        "Net",
        Style::default()
            .fg(theme::TEXT_BRIGHT())
            .add_modifier(Modifier::BOLD),
    ))];
    lines.push(Line::from(""));

    lines.push(Line::from(Span::styled(
        "        U",
        Style::default().fg(theme::TEXT_DIM()),
    )));
    push_net_face(&mut lines, Face::Up, stickers, 8);
    lines.push(Line::from(Span::styled(
        "L     F     R     B",
        Style::default().fg(theme::TEXT_DIM()),
    )));
    for row in 0..3 {
        let mut spans = Vec::new();
        for face in [Face::Left, Face::Front, Face::Right, Face::Back] {
            push_mini_row(&mut spans, face, row, stickers);
            spans.push(Span::raw(" "));
        }
        lines.push(Line::from(spans));
    }
    lines.push(Line::from(Span::styled(
        "        D",
        Style::default().fg(theme::TEXT_DIM()),
    )));
    push_net_face(&mut lines, Face::Down, stickers, 8);

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "lowercase clockwise",
        Style::default().fg(theme::TEXT_DIM()),
    )));
    lines.push(Line::from(Span::styled(
        "uppercase inverse",
        Style::default().fg(theme::TEXT_DIM()),
    )));

    frame.render_widget(Paragraph::new(lines), area);
}

fn push_net_face(
    lines: &mut Vec<Line<'static>>,
    face: Face,
    stickers: &[[Sticker; 9]; 6],
    indent: usize,
) {
    for row in 0..3 {
        let mut spans = vec![Span::raw(" ".repeat(indent))];
        push_mini_row(&mut spans, face, row, stickers);
        lines.push(Line::from(spans));
    }
}

fn push_mini_row(
    spans: &mut Vec<Span<'static>>,
    face: Face,
    row: usize,
    stickers: &[[Sticker; 9]; 6],
) {
    for col in 0..3 {
        spans.push(sticker_span(stickers[face.index()][row * 3 + col], 2));
    }
}

fn push_face_row(spans: &mut Vec<Span<'static>>, row: [Sticker; 3], width: usize) {
    for sticker in row {
        spans.push(sticker_span(sticker, width));
        spans.push(Span::raw(" "));
    }
}

fn sticker_span(sticker: Sticker, width: usize) -> Span<'static> {
    Span::styled(
        " ".repeat(width),
        Style::default().bg(sticker_color(sticker)),
    )
}

fn sticker_color(sticker: Sticker) -> Color {
    match sticker {
        Sticker::White => Color::Rgb(232, 236, 239),
        Sticker::Yellow => Color::Rgb(246, 202, 68),
        Sticker::Orange => Color::Rgb(236, 126, 42),
        Sticker::Red => Color::Rgb(212, 63, 56),
        Sticker::Green => Color::Rgb(63, 160, 92),
        Sticker::Blue => Color::Rgb(65, 115, 204),
    }
}
