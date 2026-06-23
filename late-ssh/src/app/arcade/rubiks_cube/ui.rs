use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use super::state::{
    DAILY_WIN_REWARD_CHIPS, NetTile, State, Sticker, face_for_view, net_view, oriented_face,
};
use crate::app::arcade::ui::{
    GameBottomBar, centered_rect, draw_game_frame, draw_game_overlay, keys_line, status_line,
    tip_line,
};
use crate::app::common::theme;

const MINI_STICKER_WIDTH: usize = 2;
const NET_FACE_INTERIOR: usize = MINI_STICKER_WIDTH * 3;
const NET_BOX_WIDTH: usize = NET_FACE_INTERIOR + 2;
const NET_FACE_GAP: usize = 1;
const NET_MIDDLE_INDENT: usize = NET_BOX_WIDTH + NET_FACE_GAP;

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
        tip: Some(tip_line(if state.reset_pending() {
            "Press reset again to reset today's cube.".to_string()
        } else {
            state.message().to_string()
        })),
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
        .constraints([Constraint::Length(37), Constraint::Min(40)])
        .split(content);

    draw_net(frame, columns[0], state);
    draw_cube(frame, columns[1], state);

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

    // Filled isometric: front face straight, top as a parallelogram lid on the
    // front's top edge, right face receding off the front's right edge. Painted
    // into a pixel canvas, then emitted as runs of background-colored spans so
    // the faces read as one solid block instead of detached stickers.
    const SW: i32 = 4; // sticker width in cells
    const SH: i32 = 2; // sticker height in cells
    const DX: i32 = 2; // depth step right per layer
    const DY: i32 = 2; // depth step up per layer
    const GAP: i32 = 1; // free channel between faces
    let lid_h = 3 * DY;
    let front_y = lid_h + GAP;
    let right_x = 3 * SW + GAP;
    let width = (right_x + 3 * DX) as usize;
    let height = (front_y + 3 * SH) as usize;
    let mut canvas: Vec<Vec<Option<Color>>> = vec![vec![None; width]; height];
    let mut paint = |x0: i32, y0: i32, w: i32, h: i32, color: Color| {
        for y in y0..y0 + h {
            for x in x0..x0 + w {
                if y >= 0 && (y as usize) < height && x >= 0 && (x as usize) < width {
                    canvas[y as usize][x as usize] = Some(color);
                }
            }
        }
    };

    for (r, row) in front.iter().enumerate() {
        for (c, &sticker) in row.iter().enumerate() {
            paint(
                c as i32 * SW,
                front_y + r as i32 * SH,
                SW,
                SH,
                sticker_color(sticker),
            );
        }
    }
    for (r, row) in right.iter().enumerate() {
        for (d, &sticker) in row.iter().enumerate() {
            paint(
                right_x + d as i32 * DX,
                front_y + r as i32 * SH - d as i32 * DY,
                DX,
                SH,
                sticker_color(sticker),
            );
        }
    }
    for (d, row) in top.iter().rev().enumerate() {
        for (c, &sticker) in row.iter().enumerate() {
            paint(
                c as i32 * SW + d as i32 * DX,
                lid_h - DY - d as i32 * DY,
                SW,
                DY,
                sticker_color(sticker),
            );
        }
    }

    // Two blank leads keep the cube's top edge aligned with the net's first
    // face box across the gutter (the net column spends two rows on its title).
    let mut lines = vec![Line::from(""), Line::from("")];
    for row in canvas {
        let mut spans = Vec::new();
        let mut x = 0;
        while x < width {
            let cell = row[x];
            let start = x;
            while x < width && row[x] == cell {
                x += 1;
            }
            let text = " ".repeat(x - start);
            match cell {
                Some(color) => spans.push(Span::styled(text, Style::default().bg(color))),
                None => spans.push(Span::raw(text)),
            }
        }
        lines.push(Line::from(spans));
    }

    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Left), area);
}

fn draw_net(frame: &mut Frame, area: Rect, state: &State) {
    let net = net_view(state.stickers(), state.view());
    let mut lines = vec![Line::from(Span::styled(
        "All sides",
        Style::default()
            .fg(theme::TEXT_BRIGHT())
            .add_modifier(Modifier::BOLD),
    ))];
    lines.push(Line::from(""));

    push_net_box(&mut lines, &[&net.up], NET_MIDDLE_INDENT);
    push_net_box(
        &mut lines,
        &[&net.left, &net.front, &net.right, &net.back],
        0,
    );
    push_net_box(&mut lines, &[&net.down], NET_MIDDLE_INDENT);

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "unfolded from your view; amber = front",
        Style::default().fg(theme::TEXT_DIM()),
    )));
    lines.push(Line::from(Span::styled(
        "lowercase clockwise / uppercase inverse",
        Style::default().fg(theme::TEXT_DIM()),
    )));

    frame.render_widget(Paragraph::new(lines), area);
}

fn net_border_style(slot: &str) -> Style {
    if slot == "F" {
        Style::default()
            .fg(theme::AMBER_GLOW())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_DIM())
    }
}

// Renders one horizontal strip of bordered, labeled face boxes (top edge with
// label, three sticker rows, bottom edge). Tiles in a strip share rows so they
// sit side by side. Each tile is already oriented to the current view, and is
// labeled by its viewer-relative slot (front is always F) so the controls map
// directly onto what is on screen.
fn push_net_box(lines: &mut Vec<Line<'static>>, tiles: &[&NetTile], indent: usize) {
    let gap = || Span::raw(" ".repeat(NET_FACE_GAP));

    let mut top = vec![Span::raw(" ".repeat(indent))];
    for (idx, tile) in tiles.iter().enumerate() {
        if idx > 0 {
            top.push(gap());
        }
        let style = net_border_style(tile.slot);
        top.push(Span::styled("┌──", style));
        top.push(Span::styled(tile.slot, style.add_modifier(Modifier::BOLD)));
        top.push(Span::styled("───┐", style));
    }
    lines.push(Line::from(top));

    for row in 0..3 {
        let mut spans = vec![Span::raw(" ".repeat(indent))];
        for (idx, tile) in tiles.iter().enumerate() {
            if idx > 0 {
                spans.push(gap());
            }
            let style = net_border_style(tile.slot);
            spans.push(Span::styled("│", style));
            for col in 0..3 {
                spans.push(sticker_span(tile.grid[row][col], MINI_STICKER_WIDTH));
            }
            spans.push(Span::styled("│", style));
        }
        lines.push(Line::from(spans));
    }

    let mut bottom = vec![Span::raw(" ".repeat(indent))];
    for (idx, tile) in tiles.iter().enumerate() {
        if idx > 0 {
            bottom.push(gap());
        }
        bottom.push(Span::styled("└──────┘", net_border_style(tile.slot)));
    }
    lines.push(Line::from(bottom));
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
