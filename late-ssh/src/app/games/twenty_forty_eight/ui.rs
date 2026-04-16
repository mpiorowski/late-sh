use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use super::state::State;
use crate::app::common::theme;
use crate::app::games::ui::{
    centered_rect, draw_game_frame, draw_game_overlay, info_label_value, info_tagline, key_hint,
};

pub fn draw_game(frame: &mut Frame, area: Rect, state: &State) {
    let top_tile = state
        .grid
        .iter()
        .flat_map(|row| row.iter())
        .copied()
        .max()
        .unwrap_or(0);

    let info_lines = vec![
        info_tagline("Slide. Merge. Survive."),
        Line::from(""),
        info_label_value("Score", format!("{}", state.score), theme::AMBER_GLOW()),
        info_label_value("Best", format!("{}", state.best_score), theme::SUCCESS()),
        info_label_value(
            "Best Tile",
            format!("{}", top_tile.max(2)),
            theme::TEXT_BRIGHT(),
        ),
        Line::from(""),
        key_hint("h/j/k/l", "move"),
        key_hint("r", "restart"),
        key_hint("Esc", "exit"),
    ];

    let board_area = draw_game_frame(frame, area, "2048", info_lines);

    let game_area = centered_rect(
        board_area,
        32.min(board_area.width),
        16.min(board_area.height),
    );

    let row_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Length(4),
            Constraint::Length(4),
            Constraint::Length(4),
        ])
        .split(game_area);

    for r in 0..4 {
        let col_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(8),
                Constraint::Length(8),
                Constraint::Length(8),
                Constraint::Length(8),
            ])
            .split(row_layout[r]);

        for c in 0..4 {
            let val = state.grid[r][c];
            draw_cell(frame, col_layout[c], val);
        }
    }

    if state.is_game_over {
        draw_game_overlay(
            frame,
            board_area,
            "GAME OVER",
            "No moves left",
            theme::ERROR(),
        );
    }
}

fn draw_cell(frame: &mut Frame, area: Rect, value: u32) {
    let (bg, fg) = tile_colors(value);

    let text = if value == 0 {
        String::new()
    } else {
        format!("{}", value)
    };

    let cell = Paragraph::new(vec![
        Line::from(""),
        Line::from(Span::styled(
            text,
            Style::default().fg(fg).add_modifier(Modifier::BOLD),
        )),
    ])
    .alignment(Alignment::Center)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::BORDER_DIM())),
    )
    .style(Style::default().bg(bg));

    frame.render_widget(cell, area);
}

fn tile_colors(val: u32) -> (Color, Color) {
    match val {
        0 => (ratatui::style::Color::Reset, theme::TEXT()),
        2 => (Color::Rgb(238, 228, 218), Color::Rgb(119, 110, 101)),
        4 => (Color::Rgb(237, 224, 200), Color::Rgb(119, 110, 101)),
        8 => (Color::Rgb(242, 177, 121), Color::Rgb(249, 246, 242)),
        16 => (Color::Rgb(245, 149, 99), Color::Rgb(249, 246, 242)),
        32 => (Color::Rgb(246, 124, 95), Color::Rgb(249, 246, 242)),
        64 => (Color::Rgb(246, 94, 59), Color::Rgb(249, 246, 242)),
        128 => (Color::Rgb(237, 207, 114), Color::Rgb(249, 246, 242)),
        256 => (Color::Rgb(237, 204, 97), Color::Rgb(249, 246, 242)),
        512 => (Color::Rgb(237, 200, 80), Color::Rgb(249, 246, 242)),
        1024 => (Color::Rgb(237, 197, 63), Color::Rgb(249, 246, 242)),
        2048 => (Color::Rgb(237, 194, 46), Color::Rgb(249, 246, 242)),
        _ => (Color::Rgb(60, 58, 50), Color::Rgb(249, 246, 242)),
    }
}
