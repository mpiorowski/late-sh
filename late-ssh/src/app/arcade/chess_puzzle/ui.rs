//! The Chess Puzzle screen: the shared chess board, drawn from the player's
//! side, with a side panel when the terminal is wide enough for one. Pieces
//! are always ASCII: the Arcade view carries no terminal-image frame, and
//! image pieces would linger under the solved overlay. Mouse hit-testing
//! reads the same layout the frame drew.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};
use uuid::Uuid;

use super::state::{LIVES, Outcome, State};
use crate::app::{
    arcade::ui::{
        GameBottomBar, OverlayAnchor, SHOW_GAME_BOTTOM_BAR, draw_game_frame,
        draw_game_overlay_anchored, game_content_area, keys_line, share_hints, status_line,
        tip_line,
    },
    common::theme,
    files::terminal_image::TerminalImageFrame,
    games::chess_core::{
        board_ui::{self, BoardCtx, Tier, pick_tier},
        types::{ChessColor, ChessPieceRenderMode},
    },
};

const FULL_CONTROL_HINTS_WIDTH: u16 = 100;
const PANEL_WIDTH: u16 = 30;
const PANEL_GAP: u16 = 3;
const CREDIT: &str = "Puzzles: the Lichess open database (CC0)";

/// Where the board and the side panel go inside the board area.
#[derive(Clone, Copy)]
pub(crate) struct Layout {
    pub board: Rect,
    pub tier: Tier,
    pub panel: Option<Rect>,
}

pub fn draw_game(frame: &mut Frame, area: Rect, state: &State, show_bottom_bar: bool) {
    let (reward, reward_color) = match state.reward_chips() {
        Some(chips) => (format!("{chips} chips"), theme::AMBER_GLOW()),
        None => ("none".to_string(), theme::TEXT_DIM()),
    };
    let control_hints = if area.width >= FULL_CONTROL_HINTS_WIDTH {
        vec![
            ("hjkl/↕↔", "move"),
            ("space/click", "pick up/play"),
            ("[]", "change diff"),
            ("n/r", "new/restart"),
            ("d/p", "daily/personal"),
            ("q", "exit"),
        ]
    } else {
        vec![
            ("hjkl↕↔", ""),
            ("spc", "play"),
            ("[]", "diff"),
            ("n", "new"),
            ("d/p", "mode"),
            ("q", "exit"),
        ]
    };
    let bottom = GameBottomBar {
        status: status_line(vec![
            (
                "mode",
                format!("{} {}", state.mode_label(), state.difficulty().key()),
                theme::TEXT_BRIGHT(),
            ),
            ("lives", lives_text(state.lives_left()), theme::ERROR()),
            (
                "found",
                format!("{}/{}", state.first_tries().len(), state.player_moves()),
                theme::SUCCESS(),
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
    let board_area = draw_game_frame(frame, area, "Chess Puzzle", bottom, show_bottom_bar);
    if !state.is_loaded() {
        // The day's puzzles are claimed in the database; until they land
        // (or when they could not) the tip line says so and there is no
        // board to draw.
        return;
    }
    let layout = layout(board_area);
    let ctx = BoardCtx {
        orientation: state.player(),
        cursor: state.outcome().is_none().then_some(state.cursor()),
        selected: state.selected(),
        last: state.last_move(),
        check_sq: state.check_square(),
    };
    board_ui::draw_board(
        frame,
        layout.board,
        layout.tier,
        &state.pieces(),
        &ctx,
        &state.legal_targets(),
        Uuid::nil(),
        None,
        &mut TerminalImageFrame::default(),
        ChessPieceRenderMode::Ascii,
        false,
    );
    if let Some(panel) = layout.panel {
        frame.render_widget(
            Paragraph::new(panel_lines(state)).wrap(Wrap { trim: true }),
            panel,
        );
    }

    if state.outcome() == Some(Outcome::Solved) && state.has_started() {
        let subtext = match (state.reward_chips(), state.mistakes()) {
            (Some(chips), 0) => format!("first try · {chips} chips"),
            (Some(chips), 1) => format!("1 mistake · {chips} chips"),
            (Some(chips), mistakes) => format!("{mistakes} mistakes · {chips} chips"),
            (None, _) => "no reward · n twice for another".to_string(),
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

/// The side panel: who is to move and the hint, the lives, the moves so
/// far, the solution once the puzzle is lost, and the credit.
fn panel_lines(state: &State) -> Vec<Line<'static>> {
    let heading = match state.outcome() {
        Some(Outcome::Solved) => "Solved".to_string(),
        Some(Outcome::Lost) => "Out of lives".to_string(),
        None => format!("{} to move", state.player().label()),
    };
    let rating = state.rating().expect("the panel draws a loaded board");
    let hint = match state.theme() {
        Some(theme) => format!("{} · rated {rating}", theme.label()),
        None => format!("Rated {rating}"),
    };
    let mut lines = vec![
        Line::from(Span::styled(
            heading,
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(hint, Style::default().fg(theme::AMBER()))),
        Line::from(Span::styled(
            format!("Lives {}", lives_text(state.lives_left())),
            Style::default().fg(theme::ERROR()),
        )),
        Line::default(),
        Line::from(Span::styled(
            format!(
                "{} played {}",
                state.player().other().label(),
                state.setup_san()
            ),
            Style::default().fg(theme::TEXT_MUTED()),
        )),
    ];
    if !state.played_san().is_empty() {
        lines.push(Line::from(Span::styled(
            state.played_san().join(" "),
            Style::default().fg(theme::TEXT()),
        )));
    }
    if state.outcome() == Some(Outcome::Lost) {
        lines.push(Line::default());
        lines.push(Line::from(Span::styled(
            format!("Solution: {}", state.remaining_san().join(" ")),
            Style::default().fg(theme::SUCCESS()),
        )));
    }
    lines.push(Line::default());
    lines.push(Line::from(Span::styled(
        CREDIT,
        Style::default().fg(theme::TEXT_DIM()),
    )));
    lines
}

fn lives_text(left: usize) -> String {
    (0..LIVES)
        .map(|life| if life < left { '♥' } else { '♡' })
        .collect()
}

/// The largest board that fits beside the panel, or the largest that fits
/// alone when the panel leaves too little room. The draw path and mouse
/// hit-testing both go through here so a click lands on the square drawn.
pub(crate) fn layout(board_area: Rect) -> Layout {
    let height = usize::from(board_area.height);
    let beside = board_area.width.saturating_sub(PANEL_WIDTH + PANEL_GAP);
    let with_panel = pick_tier(usize::from(beside), height);
    let fits_beside = with_panel.board_w() <= usize::from(beside) && with_panel.board_h() <= height;
    let (tier, panel_width) = if fits_beside {
        (with_panel, PANEL_WIDTH + PANEL_GAP)
    } else {
        (pick_tier(usize::from(board_area.width), height), 0)
    };
    let board_w = (tier.board_w() as u16).min(board_area.width);
    let board_h = (tier.board_h() as u16).min(board_area.height);
    let group_w = (board_w + panel_width).min(board_area.width);
    let board = Rect::new(
        board_area.x + (board_area.width - group_w) / 2,
        board_area.y + (board_area.height - board_h) / 2,
        board_w,
        board_h,
    );
    let panel = (panel_width > 0)
        .then(|| Rect::new(board.right() + PANEL_GAP, board.y, PANEL_WIDTH, board_h));
    Layout { board, tier, panel }
}

/// The square under a zero-based terminal cell, for the Arcade page `area`.
pub fn hit_test(area: Rect, orientation: ChessColor, x: u16, y: u16) -> Option<usize> {
    let board_area = game_content_area(area, true, SHOW_GAME_BOTTOM_BAR);
    let layout = layout(board_area);
    board_ui::square_at(layout.board, layout.tier, orientation, x, y)
}

#[cfg(test)]
#[path = "ui_test.rs"]
mod ui_test;
