//! The Quartets screen: four rows, each a quartet found (a bar with its
//! link and words) or four word tiles still on the wall, then the strikes
//! and the picks. On a wall out of strikes the quartets not found are
//! shown as dim bars. Mouse hit-testing reads the same layout the frame
//! drew.

use late_core::models::leaderboard::DailyPuzzle;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use super::state::{MAX_STRIKES, Outcome, Puzzle, QUARTETS, State};
use crate::app::{
    arcade::ui::{
        GameBottomBar, SHOW_GAME_BOTTOM_BAR, centered_rect, draw_game_frame, game_content_area,
        keys_line, share_hints, single_board_chips, status_line, tip_line,
    },
    common::theme,
};

const FULL_CONTROL_HINTS_WIDTH: u16 = 100;
const TILE_HEIGHT: u16 = 3;
const ROW_GAP: u16 = 1;
const TILE_GAP: u16 = 1;
/// Tile widths, from the longest word plus its margin.
const MIN_TILE_WIDTH: u16 = 10;
const MAX_TILE_WIDTH: u16 = 16;
const TILE_MARGIN: u16 = 4;
const WALL_HEIGHT: u16 = QUARTETS as u16 * (TILE_HEIGHT + ROW_GAP) - ROW_GAP;

/// Where each part goes inside the board area.
#[derive(Clone, Copy)]
pub(crate) struct WallLayout {
    pub wall: Rect,
    pub tile_width: u16,
    pub status: Rect,
}

pub fn draw_game(frame: &mut Frame, area: Rect, state: &State, show_bottom_bar: bool) {
    let control_hints = if area.width >= FULL_CONTROL_HINTS_WIDTH {
        vec![
            ("arrows", "move"),
            ("Space", "pick"),
            ("Enter", "try"),
            ("x", "clear"),
            ("Tab", "shuffle"),
            ("!", "rules"),
            ("?", "help"),
            ("`", "dashboard"),
            ("Esc", "exit"),
        ]
    } else {
        vec![
            ("Space", "pick"),
            ("Enter", "try"),
            ("Tab", "shuffle"),
            ("!", "rules"),
            ("Esc", "exit"),
        ]
    };
    let bottom = GameBottomBar {
        status: status_line(vec![
            (
                "found",
                format!("{}/{QUARTETS}", state.found().len()),
                theme::SUCCESS(),
            ),
            (
                "strikes",
                format!("{}/{MAX_STRIKES}", state.strikes()),
                theme::ERROR(),
            ),
            (
                "tries",
                state.tries().len().to_string(),
                theme::TEXT_BRIGHT(),
            ),
        ]),
        keys: keys_line(
            control_hints
                .into_iter()
                .chain(share_hints(super::share::is_ready(state)))
                .collect(),
        ),
        tip: Some(tip_line(state.message().to_string())),
    };

    let board_area = draw_game_frame(frame, area, "Quartets", bottom, show_bottom_bar);
    if let Some(puzzle) = state.puzzle() {
        let layout = layout(board_area, tile_width(puzzle));
        draw_wall(frame, layout, state, puzzle);
        frame.render_widget(
            Paragraph::new(status_text(state)).alignment(Alignment::Center),
            layout.status,
        );
    }
    if state.show_rules() {
        draw_rules_modal(frame, board_area);
    }
}

/// The widest tile the puzzle's longest word asks for.
pub(crate) fn tile_width(puzzle: &Puzzle) -> u16 {
    let longest = puzzle
        .quartets
        .iter()
        .flat_map(|quartet| quartet.words.iter())
        .map(|word| word.len() as u16)
        .max()
        .unwrap_or(0);
    (longest + TILE_MARGIN).clamp(MIN_TILE_WIDTH, MAX_TILE_WIDTH)
}

/// The wall, then a blank line and the status line, the wall centred and
/// its tiles narrowed to fit.
pub(crate) fn layout(board_area: Rect, tile_width: u16) -> WallLayout {
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(WALL_HEIGHT),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(0),
    ])
    .split(board_area);
    let gaps = (QUARTETS as u16 - 1) * TILE_GAP;
    let tile_width = tile_width.min(board_area.width.saturating_sub(gaps) / QUARTETS as u16);
    let width = (tile_width * QUARTETS as u16 + gaps).min(board_area.width);
    let wall_row = rows[1];
    WallLayout {
        wall: Rect::new(
            wall_row.x + (wall_row.width - width) / 2,
            wall_row.y,
            width,
            wall_row.height,
        ),
        tile_width,
        status: rows[3],
    }
}

/// The position on the wall of the tile under a zero-based terminal cell,
/// for the Arcade page `area`. The gaps between tiles are no tile.
pub fn hit_test(area: Rect, state: &State, x: u16, y: u16) -> Option<usize> {
    let puzzle = state.puzzle()?;
    if state.outcome() != Outcome::Playing {
        return None;
    }
    let board_area = game_content_area(area, true, SHOW_GAME_BOTTOM_BAR);
    let layout = layout(board_area, tile_width(puzzle));
    let wall = layout.wall;
    if x < wall.x || y < wall.y || x >= wall.right() || y >= wall.bottom() {
        return None;
    }
    let (dx, dy) = (x - wall.x, y - wall.y);
    let row_pitch = TILE_HEIGHT + ROW_GAP;
    let column_pitch = layout.tile_width + TILE_GAP;
    if dy % row_pitch >= TILE_HEIGHT || dx % column_pitch >= layout.tile_width {
        return None;
    }
    let row = usize::from(dy / row_pitch).checked_sub(state.found().len())?;
    let position = row * QUARTETS + usize::from(dx / column_pitch);
    (position < state.wall().len()).then_some(position)
}

/// One row of the wall: its top-left cell and full width.
fn row_rect(layout: WallLayout, row: usize) -> Rect {
    let y = layout.wall.y + row as u16 * (TILE_HEIGHT + ROW_GAP);
    Rect::new(layout.wall.x, y, layout.wall.width, TILE_HEIGHT).intersection(layout.wall)
}

fn draw_wall(frame: &mut Frame, layout: WallLayout, state: &State, puzzle: &Puzzle) {
    let found_style = Style::default().fg(Color::Black).bg(theme::SUCCESS());
    let shown_style = Style::default()
        .fg(theme::TEXT_MUTED())
        .bg(theme::BG_HIGHLIGHT());
    let mut row = 0;
    for quartet in state.found() {
        draw_bar(frame, row_rect(layout, row), puzzle, *quartet, found_style);
        row += 1;
    }
    match state.outcome() {
        Outcome::Playing => {
            for (position, word) in state.wall().iter().enumerate() {
                let rect = row_rect(layout, row + position / QUARTETS);
                let column = (position % QUARTETS) as u16;
                let tile = Rect::new(
                    rect.x + column * (layout.tile_width + TILE_GAP),
                    rect.y,
                    layout.tile_width,
                    rect.height,
                )
                .intersection(rect);
                draw_tile(
                    frame,
                    tile,
                    puzzle.word(*word),
                    state.is_picked(*word),
                    position == state.cursor(),
                );
            }
        }
        Outcome::OutOfStrikes => {
            for quartet in (0..QUARTETS as u8).filter(|quartet| !state.found().contains(quartet)) {
                draw_bar(frame, row_rect(layout, row), puzzle, quartet, shown_style);
                row += 1;
            }
        }
        Outcome::Solved => {}
    }
}

/// A quartet as a bar across the wall: the link, then its four words.
fn draw_bar(frame: &mut Frame, rect: Rect, puzzle: &Puzzle, quartet: u8, style: Style) {
    let words = puzzle.quartets[usize::from(quartet)]
        .words
        .iter()
        .map(|word| word.to_ascii_uppercase())
        .collect::<Vec<_>>()
        .join(" · ");
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                puzzle.link(quartet).to_string(),
                Style::default().add_modifier(Modifier::BOLD),
            )),
            Line::from(words),
        ])
        .alignment(Alignment::Center)
        .style(style),
        rect,
    );
}

/// A word tile: amber when picked, lit under the cursor.
fn draw_tile(frame: &mut Frame, rect: Rect, word: &str, picked: bool, cursor: bool) {
    let style = match (picked, cursor) {
        (true, true) => Style::default()
            .fg(Color::Black)
            .bg(theme::AMBER_GLOW())
            .add_modifier(Modifier::BOLD),
        (true, false) => Style::default()
            .fg(Color::Black)
            .bg(theme::AMBER())
            .add_modifier(Modifier::BOLD),
        (false, true) => Style::default()
            .fg(theme::TEXT_BRIGHT())
            .bg(theme::BG_SELECTION())
            .add_modifier(Modifier::BOLD),
        (false, false) => Style::default().fg(theme::TEXT()).bg(theme::BG_HIGHLIGHT()),
    };
    frame.render_widget(
        Paragraph::new(vec![Line::from(""), Line::from(word.to_ascii_uppercase())])
            .alignment(Alignment::Center)
            .style(style),
        rect,
    );
}

/// The strikes used and left, then the picks.
fn status_text(state: &State) -> Line<'static> {
    let mut spans = vec![Span::styled(
        "Strikes ",
        Style::default().fg(theme::TEXT_DIM()),
    )];
    for strike in 0..MAX_STRIKES {
        let (mark, color) = if strike < state.strikes() {
            ("✕ ", theme::ERROR())
        } else {
            ("○ ", theme::TEXT_DIM())
        };
        spans.push(Span::styled(mark, Style::default().fg(color)));
    }
    if state.outcome() == Outcome::Playing {
        spans.push(Span::styled(
            "   Picked ",
            Style::default().fg(theme::TEXT_DIM()),
        ));
        for pick in 0..QUARTETS {
            let (mark, color) = if pick < state.picked_count() {
                ("●", theme::AMBER())
            } else {
                ("○", theme::TEXT_DIM())
            };
            spans.push(Span::styled(mark, Style::default().fg(color)));
        }
    }
    Line::from(spans)
}

fn draw_rules_modal(frame: &mut Frame, area: Rect) {
    let chips = single_board_chips(DailyPuzzle::Quartets);
    let modal = centered_rect(area, 62.min(area.width), 21.min(area.height));
    let text = |line: String| Line::from(line);
    let rules = Paragraph::new(vec![
        Line::from(Span::styled(
            "Quartets Rules",
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        text("Sixteen words hide four quartets: four words that share a link.".to_string()),
        text("Pick four and press Enter. A quartet locks in with its link.".to_string()),
        Line::from(""),
        text(format!(
            "A miss is a strike, and says how the four split: three and one, two pairs, one pair, or all apart. {MAX_STRIKES} strikes and the wall is shown."
        )),
        text("Once three quartets are in, the last four lock in by themselves.".to_string()),
        Line::from(""),
        text(format!(
            "A solved wall pays {chips} chips. One wall for everyone, new each day."
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Space picks · x clears · Tab shuffles · ! / Esc closes",
            Style::default().fg(theme::TEXT_DIM()),
        )),
    ])
    .alignment(Alignment::Center)
    .wrap(Wrap { trim: true })
    .style(Style::default().fg(theme::TEXT_DIM()).bg(Color::Reset))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::AMBER_GLOW())),
    );
    frame.render_widget(Clear, modal);
    frame.render_widget(rules, modal);
}

#[cfg(test)]
#[path = "ui_test.rs"]
mod ui_test;
