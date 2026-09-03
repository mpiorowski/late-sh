use late_core::models::chips::Difficulty;
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};
use uuid::Uuid;

use super::{
    image::{
        ImageStatus, ImageTileGeometry, NativePuzzleImageSet, TileView, image_tile_geometry,
        tile_fragment,
    },
    state::{State, board_dimension},
};
use crate::app::{
    arcade::ui::{
        GameBottomBar, OverlayAnchor, SHOW_GAME_BOTTOM_BAR, centered_rect, draw_game_frame,
        draw_game_overlay_anchored, game_content_area, keys_line, status_line, tip_line,
    },
    common::theme,
    files::terminal_image::{TerminalImageFrame, TerminalImagePlacement, TerminalImageProtocol},
};

const NUMBER_TILE_WIDTH: u16 = 7;
const NUMBER_TILE_HEIGHT: u16 = 3;
const FULL_CONTROL_HINTS_WIDTH: u16 = 89;

pub fn draw_game(
    frame: &mut Frame,
    area: Rect,
    state: &State,
    show_bottom_bar: bool,
    terminal_image_protocol: Option<TerminalImageProtocol>,
    terminal_images: &mut TerminalImageFrame,
) {
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
    let tip = match state.image_status() {
        ImageStatus::Loading => "Loading art; numbered fallback remains playable.".to_string(),
        ImageStatus::Failed if area.width >= FULL_CONTROL_HINTS_WIDTH => {
            "Art unavailable; numbered fallback active. Press i twice to retry.".to_string()
        }
        ImageStatus::Failed => "Art unavailable; i twice to retry.".to_string(),
        ImageStatus::Numbered | ImageStatus::Ready => state.message().to_string(),
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
        tip: Some(tip_line(tip)),
    };
    let board_area = draw_game_frame(frame, area, "Sliding Puzzle", bottom, show_bottom_bar);
    let dimension = board_dimension(state.difficulty()) as u16;
    let view = state.tile_view();
    let show_image_numbers = !state.is_solved();
    let Some((grid, geometry)) = grid_layout(board_area, state.difficulty(), view) else {
        frame.render_widget(
            Paragraph::new("Terminal too small for Sliding Puzzle").alignment(Alignment::Center),
            board_area,
        );
        return;
    };

    let native_tiles = native_placement(area, state, terminal_image_protocol, show_bottom_bar)
        .filter(|(_, placement)| *placement == grid)
        .map(|(images, _)| images);
    if let Some(images) = native_tiles {
        frame.render_widget(Clear, grid);
        for (index, image) in (0..state.board().len()).filter_map(|index| {
            images
                .cell_image(state.board(), index)
                .map(|image| (index, image))
        }) {
            terminal_images.push(TerminalImagePlacement {
                message_id: native_tile_message_id(index, image.cache_key()),
                area: tile_area(grid, geometry, dimension, index),
                data: image.clone(),
            });
        }
    } else {
        for (index, tile) in state.board().iter().copied().enumerate() {
            let tile_area = tile_area(grid, geometry, dimension, index);
            if tile == 0 {
                frame.render_widget(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(theme::AMBER_DIM())),
                    tile_area,
                );
            } else if view == TileView::Image
                && let Some(mut fragment) = state.image_preview().and_then(|preview| {
                    tile_fragment(preview, usize::from(dimension), tile, geometry)
                })
            {
                if show_image_numbers {
                    add_image_tile_number(&mut fragment, tile, geometry);
                }
                frame.render_widget(Paragraph::new(fragment), tile_area);
            } else {
                draw_numbered_tile(frame, tile_area, tile);
            }
        }
    }

    if state.is_solved() && state.has_started() && native_tiles.is_none() {
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

fn add_image_tile_number(fragment: &mut [Line<'static>], tile: u8, geometry: ImageTileGeometry) {
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

fn grid_layout(
    board_area: Rect,
    difficulty: Difficulty,
    view: TileView,
) -> Option<(Rect, ImageTileGeometry)> {
    let dimension = board_dimension(difficulty) as u16;
    let geometry = match view {
        TileView::Numbered => ImageTileGeometry {
            width: NUMBER_TILE_WIDTH,
            height: NUMBER_TILE_HEIGHT,
        },
        TileView::Image => image_tile_geometry(board_area, difficulty)?,
    };
    let width = dimension.saturating_mul(geometry.width);
    let height = dimension.saturating_mul(geometry.height);
    (board_area.width >= width && board_area.height >= height)
        .then(|| (centered_rect(board_area, width, height), geometry))
}

fn tile_area(grid: Rect, geometry: ImageTileGeometry, dimension: u16, index: usize) -> Rect {
    let row = index as u16 / dimension;
    let column = index as u16 % dimension;
    Rect::new(
        grid.x + column * geometry.width,
        grid.y + row * geometry.height,
        geometry.width,
        geometry.height,
    )
}

fn native_tiles_fit_grid(geometry: ImageTileGeometry, grid: Rect, dimension: u16) -> bool {
    geometry.width.saturating_mul(dimension) == grid.width
        && geometry.height.saturating_mul(dimension) == grid.height
}

fn native_tile_message_id(destination: usize, cache_key: u64) -> Uuid {
    let destination = u64::try_from(destination).unwrap_or(u64::MAX);
    Uuid::from_u128(
        0x5a13_1d1e_5a13_1d1e_0000_0000_0000_0000
            ^ (u128::from(destination) << 64)
            ^ u128::from(cache_key),
    )
}

fn native_tiles_placement_area(
    area: Rect,
    difficulty: Difficulty,
    show_bottom_bar: bool,
    images: &NativePuzzleImageSet,
) -> Option<Rect> {
    let board_area = game_content_area(area, true, show_bottom_bar);
    let (grid, _) = grid_layout(board_area, difficulty, TileView::Image)?;
    native_tiles_fit_grid(images.geometry(), grid, board_dimension(difficulty) as u16)
        .then_some(grid)
}

/// The native cell set this game will place and the grid it will land on, or
/// `None` when the frame falls back to Chafa fragments or numbered tiles.
/// Both the draw path and the pre-frame raster wipe go through here so they
/// cannot disagree about whether a raster is on screen.
fn native_placement(
    arcade_area: Rect,
    state: &State,
    protocol: Option<TerminalImageProtocol>,
    show_bottom_bar: bool,
) -> Option<(&NativePuzzleImageSet, Rect)> {
    let protocol = protocol?;
    // `display_native_tiles` already answers `None` outside the image view.
    let images = state.display_native_tiles()?;
    if !images.supports_protocol(protocol) {
        return None;
    }
    let board = state.board();
    if !(0..board.len()).all(|index| images.cell_image(board, index).is_some()) {
        return None;
    }
    let grid =
        native_tiles_placement_area(arcade_area, state.difficulty(), show_bottom_bar, images)?;
    Some((images, grid))
}

/// Identity of the native raster this game will place into `arcade_area` on
/// the next frame: its grid rect combined with the content of every cell.
/// `None` when it will place nothing.
///
/// The pre-frame wipe that consumes this runs *before* `terminal.draw`, so it
/// cannot observe what the frame drew and has to predict it. Keeping the
/// prediction here — beside the code it predicts, and sharing
/// `native_placement` with it — is what stops the two from drifting apart;
/// the render loop only decides whether to ask.
pub(crate) fn persistent_raster_tag(
    arcade_area: Rect,
    state: &State,
    protocol: Option<TerminalImageProtocol>,
) -> Option<u64> {
    let (images, placement) = native_placement(arcade_area, state, protocol, SHOW_GAME_BOTTOM_BAR)?;
    let board = state.board();
    // An opaque set draws the same pixels wherever a cell lands, so its own
    // identity is enough. A set that is not opaque shows whatever sits under
    // the transparent pixels, which depends on the arrangement.
    let cache_key = if images.is_opaque() {
        images.cache_key()
    } else {
        images.cache_key_for_board(board)?
    };
    Some(crate::app::files::terminal_image::persistent_raster_tag(
        placement, cache_key,
    ))
}

fn hit_layout(
    area: Rect,
    difficulty: Difficulty,
    view: TileView,
) -> Option<(Rect, ImageTileGeometry)> {
    grid_layout(
        game_content_area(area, true, SHOW_GAME_BOTTOM_BAR),
        difficulty,
        view,
    )
}

pub fn hit_test(
    area: Rect,
    difficulty: Difficulty,
    view: TileView,
    x: u16,
    y: u16,
) -> Option<usize> {
    let (grid, geometry) = hit_layout(area, difficulty, view)?;
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
