use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use super::state::{DAILY_WIN_REWARD_CHIPS, Face, State, Sticker, face_for_view, oriented_face};
use crate::app::arcade::ui::{
    GameBottomBar, centered_rect, draw_game_frame, draw_game_overlay, keys_line, status_line,
    tip_line,
};
use crate::app::common::theme;

const MINI_STICKER_WIDTH: usize = 2;
const MINI_FACE_WIDTH: usize = MINI_STICKER_WIDTH * 3;
const MINI_FACE_GAP: usize = 1;
const MINI_FACE_STRIDE: usize = MINI_FACE_WIDTH + MINI_FACE_GAP;
const NET_MIDDLE_FACE_INDENT: usize = MINI_FACE_STRIDE;

pub fn draw_game(frame: &mut Frame, area: Rect, state: &State, show_bottom_bar: bool) {
    let bottom = GameBottomBar {
        status: status_line(vec![
            ("daily", state.daily_label(), theme::SUCCESS()),
            (
                "reward",
                format!("{DAILY_WIN_REWARD_CHIPS} chips"),
                theme::AMBER_GLOW(),
            ),
            ("view", state.view_label(), theme::TEXT_BRIGHT()),
        ]),
        keys: keys_line(vec![
            ("u/d/l/r/f/b", "turn"),
            ("Shift", "inverse"),
            ("s/0", "reset daily"),
            ("v/arrows", "rotate view"),
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

    if state.is_solved() && state.has_started() {
        draw_game_overlay(
            frame,
            board_area,
            "SOLVED",
            &format!("{DAILY_WIN_REWARD_CHIPS} chips"),
            theme::SUCCESS(),
        );
    }
}

fn draw_cube(frame: &mut Frame, area: Rect, state: &State) {
    let view = state.view();
    let (top_face, front_face, right_face) = face_for_view(view);
    let top = oriented_face(state.stickers(), top_face, view);
    let front = oriented_face(state.stickers(), front_face, view);
    let right = oriented_face(state.stickers(), right_face, view);

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

    for (row, stickers) in top.iter().enumerate() {
        let mut spans = Vec::new();
        spans.push(Span::raw(" ".repeat(12 - row * 2)));
        push_face_row(&mut spans, *stickers, 4, true);
        lines.push(Line::from(spans));
    }

    for (row, stickers) in front.iter().enumerate() {
        let mut spans = Vec::new();
        spans.push(Span::raw("      "));
        push_face_row(&mut spans, *stickers, 4, false);
        spans.push(Span::raw(" ".repeat(row * 2)));
        push_face_row(&mut spans, right[row], 4, false);
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

    push_net_label_line(&mut lines, &[(NET_MIDDLE_FACE_INDENT, Face::Up)]);
    push_net_face(&mut lines, Face::Up, stickers, NET_MIDDLE_FACE_INDENT);
    push_net_label_line(
        &mut lines,
        &[
            (0, Face::Left),
            (MINI_FACE_STRIDE, Face::Front),
            (MINI_FACE_STRIDE * 2, Face::Right),
            (MINI_FACE_STRIDE * 3, Face::Back),
        ],
    );
    for row in 0..3 {
        let mut spans = Vec::new();
        for (idx, face) in [Face::Left, Face::Front, Face::Right, Face::Back]
            .into_iter()
            .enumerate()
        {
            push_mini_row(&mut spans, face, row, stickers);
            if idx < 3 {
                spans.push(Span::raw(" ".repeat(MINI_FACE_GAP)));
            }
        }
        lines.push(Line::from(spans));
    }
    push_net_label_line(&mut lines, &[(NET_MIDDLE_FACE_INDENT, Face::Down)]);
    push_net_face(&mut lines, Face::Down, stickers, NET_MIDDLE_FACE_INDENT);

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

fn push_net_label_line(lines: &mut Vec<Line<'static>>, labels: &[(usize, Face)]) {
    let mut text = String::new();
    for (face_start, face) in labels {
        let label_start = face_start + MINI_FACE_WIDTH / 2;
        if text.len() < label_start {
            text.push_str(&" ".repeat(label_start - text.len()));
        }
        text.push_str(face.label());
    }
    lines.push(Line::from(Span::styled(
        text,
        Style::default().fg(theme::TEXT_DIM()),
    )));
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
        spans.push(sticker_span(
            stickers[face.index()][row * 3 + col],
            MINI_STICKER_WIDTH,
        ));
    }
}

fn push_face_row(
    spans: &mut Vec<Span<'static>>,
    row: [Sticker; 3],
    width: usize,
    trailing_gap: bool,
) {
    for (idx, sticker) in row.into_iter().enumerate() {
        spans.push(sticker_span(sticker, width));
        if trailing_gap || idx < 2 {
            spans.push(Span::raw(" "));
        }
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
