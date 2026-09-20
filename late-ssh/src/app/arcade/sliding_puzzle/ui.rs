use late_core::models::chips::Difficulty;
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use super::{
    art::{TileGeometry, tile_fragment},
    state::{ArtStatus, State, board_dimension},
};
use crate::app::{
    arcade::ui::{
        GameBottomBar, OverlayAnchor, SHOW_GAME_BOTTOM_BAR, centered_rect, draw_game_frame,
        draw_game_overlay_anchored, game_content_area, keys_line, status_line, tip_line,
    },
    common::theme,
};

const NUMBERED_TILE_GEOMETRY: TileGeometry = TileGeometry {
    width: 7,
    height: 3,
};
const FULL_CONTROL_HINTS_WIDTH: u16 = 89;

pub fn draw_game(frame: &mut Frame, area: Rect, state: &State, show_bottom_bar: bool) {
    let (reward, reward_color) = match state.reward_chips() {
        Some(chips) => (format!("{chips} chips"), theme::AMBER_GLOW()),
        None => ("none".to_string(), theme::TEXT_DIM()),
    };
    let control_hints = if area.width >= FULL_CONTROL_HINTS_WIDTH {
        vec![
            ("click/hjkl/↕↔", "move"),
            ("i", "art"),
            ("[]", "change diff"),
            ("n/r", "new/reset"),
            ("d/p", "daily/personal"),
            ("q", "exit"),
        ]
    } else {
        vec![
            ("hjkl↕↔", ""),
            ("i", "art"),
            ("[]", "diff"),
            ("r", "reset"),
            ("d/p", "mode"),
            ("q", "exit"),
        ]
    };
    let board_area = game_content_area(area, true, show_bottom_bar);
    let difficulty = state.difficulty();
    let art_geometry = state.art_tile_geometry();
    let layout = board_layout(board_area, difficulty, art_geometry);
    let art_fits = layout.is_some_and(|(_, geometry)| Some(geometry) == art_geometry);
    let tip = match state.art_status() {
        ArtStatus::Loading => "Loading today's art; numbered tiles until it lands.".to_string(),
        ArtStatus::Empty => {
            "No gallery art yet: hang a piece on the Artboard and it shows here tomorrow."
                .to_string()
        }
        ArtStatus::Failed if area.width >= FULL_CONTROL_HINTS_WIDTH => {
            "Art unavailable; numbered fallback active. Press i twice to retry.".to_string()
        }
        ArtStatus::Failed => "Art unavailable; i twice to retry.".to_string(),
        ArtStatus::Ready if !art_fits => match art_geometry {
            Some(geometry) => format!(
                "Today's art needs a {}×{} board; numbered tiles until the terminal grows.",
                geometry.width * board_dimension(difficulty) as u16,
                geometry.height * board_dimension(difficulty) as u16
            ),
            None => state.message().to_string(),
        },
        ArtStatus::Numbered | ArtStatus::Ready => state.message().to_string(),
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
        keys: keys_line(
            control_hints
                .into_iter()
                .chain(crate::app::arcade::ui::share_hints(super::share::is_ready(
                    state,
                )))
                .collect(),
        ),
        tip: Some(tip_line(tip)),
    };
    let board_area = draw_game_frame(frame, area, "Sliding Puzzle", bottom, show_bottom_bar);
    let dimension = board_dimension(difficulty) as u16;
    let Some((grid, geometry)) = layout else {
        frame.render_widget(
            Paragraph::new("Terminal too small for Sliding Puzzle").alignment(Alignment::Center),
            board_area,
        );
        return;
    };
    let art = if art_fits { state.art_grid() } else { None };
    let show_art_numbers = !state.is_solved();

    for (index, tile) in state.board().iter().copied().enumerate() {
        let tile_area = tile_area(grid, geometry, dimension, index);
        if tile == 0 {
            frame.render_widget(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme::AMBER_DIM())),
                tile_area,
            );
        } else if let Some(mut fragment) =
            art.and_then(|grid| tile_fragment(grid, difficulty, tile))
        {
            if show_art_numbers {
                add_art_tile_number(&mut fragment, tile, geometry);
            }
            frame.render_widget(Paragraph::new(fragment), tile_area);
        } else {
            draw_numbered_tile(frame, tile_area, tile);
        }
    }

    // The credit sits under the grid when the board has a spare row; the
    // tip line stays the game's own messages.
    if let Some(credit) = state.art_credit()
        && art.is_some()
        && grid.bottom() < board_area.bottom()
    {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                credit,
                Style::default().fg(theme::TEXT_DIM()),
            )))
            .alignment(Alignment::Center),
            Rect::new(board_area.x, grid.bottom(), board_area.width, 1),
        );
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

/// A faint tile number over the art while the board is unsolved: sparse
/// ASCII pieces have tiles that look alike, and the number is what keeps
/// them playable.
fn add_art_tile_number(fragment: &mut [Line<'static>], tile: u8, geometry: TileGeometry) {
    let Some(line) = fragment.get_mut(usize::from(geometry.height / 2)) else {
        return;
    };
    let label = tile.to_string();
    let label_width = label.len();
    let start = usize::from(geometry.width).saturating_sub(label_width) / 2;
    let end = start + label_width;
    if line.spans.len() < end {
        return;
    }
    for (span, digit) in line.spans[start..end].iter_mut().zip(label.chars()) {
        let style = span
            .style
            .fg(theme::AMBER_DIM())
            .remove_modifier(Modifier::BOLD);
        *span = Span::styled(digit.to_string(), style);
    }
}

fn draw_numbered_tile(frame: &mut Frame, tile_area: Rect, tile: u8) {
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

/// The grid the board draws into and its tile size: the art's tiles when
/// the art view has a piece that fits the area, numbered tiles otherwise.
/// The draw path and mouse hit-testing both go through here so a click
/// lands on the tile the frame drew.
pub(crate) fn board_layout(
    board_area: Rect,
    difficulty: Difficulty,
    art: Option<TileGeometry>,
) -> Option<(Rect, TileGeometry)> {
    let dimension = board_dimension(difficulty) as u16;
    let fit = |geometry: TileGeometry| {
        let width = dimension.saturating_mul(geometry.width);
        let height = dimension.saturating_mul(geometry.height);
        (board_area.width >= width && board_area.height >= height)
            .then(|| (centered_rect(board_area, width, height), geometry))
    };
    match art.and_then(fit) {
        Some(layout) => Some(layout),
        None => fit(NUMBERED_TILE_GEOMETRY),
    }
}

fn tile_area(grid: Rect, geometry: TileGeometry, dimension: u16, index: usize) -> Rect {
    let row = index as u16 / dimension;
    let column = index as u16 % dimension;
    Rect::new(
        grid.x + column * geometry.width,
        grid.y + row * geometry.height,
        geometry.width,
        geometry.height,
    )
}

pub fn hit_test(
    area: Rect,
    difficulty: Difficulty,
    art: Option<TileGeometry>,
    x: u16,
    y: u16,
) -> Option<usize> {
    let board_area = game_content_area(area, true, SHOW_GAME_BOTTOM_BAR);
    let (grid, geometry) = board_layout(board_area, difficulty, art)?;
    if x < grid.x
        || x >= grid.x.saturating_add(grid.width)
        || y < grid.y
        || y >= grid.y.saturating_add(grid.height)
    {
        return None;
    }

    let column = (x - grid.x) / geometry.width;
    let row = (y - grid.y) / geometry.height;
    Some((row * board_dimension(difficulty) as u16 + column) as usize)
}

#[cfg(test)]
#[path = "ui_test.rs"]
mod ui_test;
