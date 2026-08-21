use late_core::models::chips::Difficulty;
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use super::state::{State, board_dimension};
use crate::app::{
    arcade::ui::{
        GameBottomBar, OverlayAnchor, centered_rect, draw_game_frame, draw_game_overlay_anchored,
        game_content_area, keys_line, status_line, tip_line,
    },
    common::theme,
};

const TILE_WIDTH: u16 = 7;
const TILE_HEIGHT: u16 = 3;
const FULL_CONTROL_HINTS_WIDTH: u16 = 81;

pub fn draw_game(frame: &mut Frame, area: Rect, state: &State, show_bottom_bar: bool) {
    let (reward, reward_color) = match state.reward_chips() {
        Some(chips) => (format!("{chips} chips"), theme::AMBER_GLOW()),
        None => ("none".to_string(), theme::TEXT_DIM()),
    };
    let control_hints = if area.width >= FULL_CONTROL_HINTS_WIDTH {
        vec![
            ("click/hjkl/↕↔", "move"),
            ("[]", "change diff"),
            ("n/r", "new/reset"),
            ("d/p", "daily/personal"),
            ("q", "exit"),
        ]
    } else {
        vec![
            ("hjkl↕↔", ""),
            ("[]", "change diff"),
            ("r", "reset"),
            ("d/p", "mode"),
            ("q", "exit"),
        ]
    };
    let bottom = GameBottomBar {
        status: status_line(vec![
            (
                "mode",
                format!("{} {}", state.mode_label(), state.difficulty_label()),
                theme::TEXT_BRIGHT(),
            ),
            ("moves", state.moves().to_string(), theme::SUCCESS()),
            ("reward", reward, reward_color),
        ]),
        keys: keys_line(control_hints),
        tip: Some(tip_line(state.message().to_string())),
    };
    let board_area = draw_game_frame(frame, area, "Sliding Puzzle", bottom, show_bottom_bar);
    let dimension = board_dimension(state.difficulty()) as u16;
    let Some(grid) = grid_rect(board_area, state.difficulty()) else {
        frame.render_widget(
            Paragraph::new("Terminal too small for Sliding Puzzle").alignment(Alignment::Center),
            board_area,
        );
        return;
    };

    for (index, tile) in state.board().iter().copied().enumerate() {
        let row = index as u16 / dimension;
        let column = index as u16 % dimension;
        let tile_area = Rect::new(
            grid.x + column * TILE_WIDTH,
            grid.y + row * TILE_HEIGHT,
            TILE_WIDTH,
            TILE_HEIGHT,
        );
        if tile == 0 {
            frame.render_widget(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme::AMBER_DIM())),
                tile_area,
            );
        } else {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    tile.to_string(),
                    Style::default()
                        .fg(theme::TEXT_BRIGHT())
                        .add_modifier(Modifier::BOLD),
                )))
                .alignment(Alignment::Center)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(theme::AMBER())),
                ),
                tile_area,
            );
        }
    }

    if state.is_solved() && state.has_started() {
        let subtext = match state.reward_chips() {
            Some(chips) => format!("{} moves · {chips} chips", state.moves()),
            None => format!("{} moves · no reward · n for new", state.moves()),
        };
        draw_game_overlay_anchored(
            frame,
            board_area,
            "SOLVED",
            &subtext,
            theme::SUCCESS(),
            OverlayAnchor::Top,
        );
    }
}

fn grid_rect(board_area: Rect, difficulty: Difficulty) -> Option<Rect> {
    let dimension = board_dimension(difficulty) as u16;
    let width = dimension.saturating_mul(TILE_WIDTH);
    let height = dimension.saturating_mul(TILE_HEIGHT);
    (board_area.width >= width && board_area.height >= height)
        .then(|| centered_rect(board_area, width, height))
}

fn hit_area(area: Rect, difficulty: Difficulty) -> Option<Rect> {
    grid_rect(game_content_area(area, true, true), difficulty)
}

pub fn hit_test(area: Rect, difficulty: Difficulty, x: u16, y: u16) -> Option<usize> {
    let grid = hit_area(area, difficulty)?;
    if x < grid.x
        || x >= grid.x.saturating_add(grid.width)
        || y < grid.y
        || y >= grid.y.saturating_add(grid.height)
    {
        return None;
    }

    let column = (x - grid.x) / TILE_WIDTH;
    let row = (y - grid.y) / TILE_HEIGHT;
    Some((row * board_dimension(difficulty) as u16 + column) as usize)
}

#[cfg(test)]
#[path = "ui_test.rs"]
mod ui_test;
