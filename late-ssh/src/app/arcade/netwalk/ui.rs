//! The Netwalk board: one tile per cell, drawn as its connections at the
//! largest size the terminal fits. Lit tiles are green, the rest dim, the
//! source amber; a locked tile draws in heavy lines, so the lock reads on
//! every palette and terminal. Mouse hit-testing reads the same layout the
//! frame drew.

use std::cmp::Ordering;

use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};

use super::state::{Side, State, source_index};
use crate::app::{
    arcade::ui::{
        GameBottomBar, OverlayAnchor, SHOW_GAME_BOTTOM_BAR, centered_rect, draw_game_frame,
        draw_game_overlay_anchored, game_content_area, keys_line, share_hints, status_line,
        tip_line,
    },
    common::theme,
};

const FULL_CONTROL_HINTS_WIDTH: u16 = 100;

/// A tile's footprint in terminal cells. Odd both ways, so a tile has a
/// centre cell with its arms around it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TileSize {
    pub width: u16,
    pub height: u16,
}

/// The tile sizes, largest first; the board takes the largest that fits.
/// At 5x3 every arm is drawn; at 3x1 only the sideways ones, the centre
/// glyph carrying up and down; at 1x1 the glyph is the tile.
const TILE_SIZES: [TileSize; 3] = [
    TileSize {
        width: 5,
        height: 3,
    },
    TileSize {
        width: 3,
        height: 1,
    },
    TileSize {
        width: 1,
        height: 1,
    },
];

/// One line weight: the glyph for each connection mask (N=1, E=2, S=4,
/// W=8) and the arms that run out to the tile's edges.
struct Strokes {
    glyphs: [char; 16],
    horizontal: char,
    vertical: char,
}

const LIGHT: Strokes = Strokes {
    glyphs: [
        ' ', '╵', '╶', '└', '╷', '│', '┌', '├', '╴', '┘', '─', '┴', '┐', '┤', '┬', '┼',
    ],
    horizontal: '─',
    vertical: '│',
};

/// A locked tile.
const HEAVY: Strokes = Strokes {
    glyphs: [
        ' ', '╹', '╺', '┗', '╻', '┃', '┏', '┣', '╸', '┛', '━', '┻', '┓', '┫', '┳', '╋',
    ],
    horizontal: '━',
    vertical: '┃',
};

pub fn draw_game(frame: &mut Frame, area: Rect, state: &State, show_bottom_bar: bool) {
    let (reward, reward_color) = match state.reward_chips() {
        Some(chips) => (format!("{chips} chips"), theme::AMBER_GLOW()),
        None => ("none".to_string(), theme::TEXT_DIM()),
    };
    let control_hints = if area.width >= FULL_CONTROL_HINTS_WIDTH {
        vec![
            ("hjkl/↕↔", "move"),
            ("space/x/click", "turn"),
            ("f", "lock"),
            ("[]", "change diff"),
            ("n/r", "new/reset"),
            ("d/p", "daily/personal"),
            ("q", "exit"),
        ]
    } else {
        vec![
            ("hjkl↕↔", ""),
            ("spc/x", "turn"),
            ("f", "lock"),
            ("[]", "diff"),
            ("r", "reset"),
            ("d/p", "mode"),
            ("q", "exit"),
        ]
    };
    let lit = state.lit();
    let lit_count = lit.iter().filter(|lit| **lit).count();
    let bottom = GameBottomBar {
        status: status_line(vec![
            (
                "mode",
                format!("{} {}", state.mode_label(), state.difficulty_label()),
                theme::TEXT_BRIGHT(),
            ),
            ("moves", state.moves().to_string(), theme::SUCCESS()),
            (
                "lit",
                format!("{lit_count}/{}", lit.len()),
                theme::TEXT_BRIGHT(),
            ),
            ("reward", reward, reward_color),
        ]),
        keys: keys_line(
            control_hints
                .into_iter()
                .chain(share_hints(super::share::is_ready(state)))
                .collect(),
        ),
        tip: Some(tip_line(state.message().to_string())),
    };
    let board_area = draw_game_frame(frame, area, "Netwalk", bottom, show_bottom_bar);
    let dimension = state.dimension();
    let Some((grid, size)) = board_layout(board_area, dimension) else {
        frame.render_widget(
            Paragraph::new("Terminal too small for Netwalk").alignment(Alignment::Center),
            board_area,
        );
        return;
    };

    let source = source_index(dimension);
    let cursor = state.cursor();
    for (index, mask) in state.board().iter().copied().enumerate() {
        let style = tile_style(lit[index], index == source, index == cursor);
        let lines: Vec<Line<'static>> =
            tile_rows(mask, size, index == source, state.is_locked(index))
                .into_iter()
                .map(|row| Line::from(Span::styled(row, style)))
                .collect();
        frame.render_widget(
            Paragraph::new(lines),
            tile_area(grid, size, dimension, index),
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

/// One tile's text at `size`, top row first: the tile's glyph in the
/// centre and, where the size has room, its connections drawn out to the
/// tile's edges to meet the neighbours'. With arms on both axes the arms
/// carry the direction, so the centre marks what the tile is instead: the
/// source `■`, an end of the network `▣`.
pub(crate) fn tile_rows(mask: u8, size: TileSize, is_source: bool, locked: bool) -> Vec<String> {
    let strokes = if locked { &HEAVY } else { &LIGHT };
    let width = usize::from(size.width);
    let height = usize::from(size.height);
    let (mid_x, mid_y) = (width / 2, height / 2);
    let roomy = width >= 3 && height >= 3;
    let centre = match (roomy, is_source, mask.count_ones()) {
        (true, true, _) => '■',
        (true, false, 1) => '▣',
        _ => strokes.glyphs[usize::from(mask)],
    };
    let arm = |side: Side, stroke: char| {
        if mask & side.bit() != 0 { stroke } else { ' ' }
    };
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| match (x.cmp(&mid_x), y.cmp(&mid_y)) {
                    (Ordering::Equal, Ordering::Equal) => centre,
                    (Ordering::Less, Ordering::Equal) => arm(Side::West, strokes.horizontal),
                    (Ordering::Greater, Ordering::Equal) => arm(Side::East, strokes.horizontal),
                    (Ordering::Equal, Ordering::Less) => arm(Side::North, strokes.vertical),
                    (Ordering::Equal, Ordering::Greater) => arm(Side::South, strokes.vertical),
                    (Ordering::Less | Ordering::Greater, Ordering::Less | Ordering::Greater) => ' ',
                })
                .collect::<String>()
        })
        .collect()
}

/// A tile's colour: the source amber, lit tiles green, the rest dim. Under
/// the cursor it sits on the selection fill, stepped until it reads there.
pub(crate) fn tile_style(lit: bool, is_source: bool, selected: bool) -> Style {
    let color = match (is_source, lit) {
        (true, _) => theme::AMBER(),
        (false, true) => theme::SUCCESS(),
        (false, false) => theme::TEXT_DIM(),
    };
    if selected {
        Style::default()
            .fg(theme::legible_on_selection(color))
            .bg(theme::BG_SELECTION())
    } else {
        Style::default().fg(color)
    }
}

/// The grid the board draws into and its tile size: the largest size whose
/// board fits the area. The draw path and mouse hit-testing both go through
/// here so a click lands on the tile the frame drew.
pub(crate) fn board_layout(board_area: Rect, dimension: usize) -> Option<(Rect, TileSize)> {
    let dimension = dimension as u16;
    TILE_SIZES.iter().copied().find_map(|size| {
        let width = dimension.saturating_mul(size.width);
        let height = dimension.saturating_mul(size.height);
        (board_area.width >= width && board_area.height >= height)
            .then(|| (centered_rect(board_area, width, height), size))
    })
}

fn tile_area(grid: Rect, size: TileSize, dimension: usize, index: usize) -> Rect {
    let row = (index / dimension) as u16;
    let column = (index % dimension) as u16;
    Rect::new(
        grid.x + column * size.width,
        grid.y + row * size.height,
        size.width,
        size.height,
    )
}

/// The tile under a zero-based terminal cell, for the Arcade page `area`.
pub fn hit_test(area: Rect, dimension: usize, x: u16, y: u16) -> Option<usize> {
    let board_area = game_content_area(area, true, SHOW_GAME_BOTTOM_BAR);
    let (grid, size) = board_layout(board_area, dimension)?;
    if x < grid.x || x >= grid.right() || y < grid.y || y >= grid.bottom() {
        return None;
    }
    let column = usize::from((x - grid.x) / size.width);
    let row = usize::from((y - grid.y) / size.height);
    Some(row * dimension + column)
}

#[cfg(test)]
#[path = "ui_test.rs"]
mod ui_test;
