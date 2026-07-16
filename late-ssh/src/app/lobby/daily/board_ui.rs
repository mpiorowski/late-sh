//! Full-screen daily-match board (`Screen::DailyMatch`). Shared chrome
//! (loading, result banners, key hints) plus per-game rendering: chess wraps
//! the shared `chess_core` renderer here, battleship draws its two grids in
//! `battleship_ui`, connect4 its board in `connect4_ui`. Entered only from
//! the Daily Games modal.

use chrono::Utc;
use late_core::models::daily_match::DailyMatch;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};
use uuid::Uuid;

use crate::app::{
    common::theme,
    files::terminal_image::{TerminalImageFrame, TerminalImageProtocol},
    games::chess_core::{
        board_ui::{self, BoardCtx, pick_tier},
        types::{ChessColor, ChessPiece, ChessPieceKind, ChessPieceRenderMode, piece_glyph},
    },
    lobby::daily::state::{
        ChessDetail, DailyBoardState, DailyGameDetail, DailyMatchDetail, DailyState,
        format_deadline,
    },
};

const INFO_SIDEBAR_WIDTH: u16 = 24;
const INFO_SIDEBAR_MIN_WIDTH: u16 = 92;

/// Below this height the chat pane is dropped and the board keeps the whole
/// screen; the pane itself takes a third of the height within these bounds.
const CHAT_MIN_AREA_HEIGHT: u16 = 27;
const CHAT_MIN_HEIGHT: u16 = 9;
const CHAT_MAX_HEIGHT: u16 = 13;

// ── Cell sizing for the grid games ────────────────────────────────────
//
// Battleship, reversi, and checkers draw plain character grids; each
// renderer picks the biggest cell footprint its area affords — the same
// idea as the chess renderer's `Tier`. The mouse hit-test never sees the
// tier: it derives the cell size from the render-recorded
// `target_geometry` rect, which is always an exact multiple of the grid.

#[derive(Clone, Copy)]
pub(super) struct CellTier {
    /// Terminal columns per board cell.
    pub cw: u16,
    /// Terminal rows per board cell.
    pub ch: u16,
}

impl CellTier {
    /// The sub-row of a cell that carries the glyph (and the row label):
    /// the middle row, rounding up for even heights — glyphs hang low in
    /// their character box, so the upper-middle row reads as centred.
    pub fn glyph_sub(self) -> u16 {
        (self.ch - 1) / 2
    }
}

/// Biggest first; the last is the cramped fallback. The big tier is 4 wide
/// so the 2-wide piece art centres exactly.
const CELL_TIERS: [CellTier; 2] = [CellTier { cw: 4, ch: 2 }, CellTier { cw: 3, ch: 1 }];

/// The biggest cell tier whose full board layout `fits` the area.
pub(super) fn pick_cell_tier(fits: impl Fn(CellTier) -> bool) -> CellTier {
    CELL_TIERS
        .into_iter()
        .find(|tier| fits(*tier))
        .unwrap_or(CELL_TIERS[CELL_TIERS.len() - 1])
}

/// `glyph` centered in a `width`-column cell.
pub(super) fn cell_text(glyph: char, width: u16) -> String {
    format!("{glyph:^0$}", width as usize)
}

// The one piece shape: a compact two-row square stone for the 2-row tier,
// coloured per side/role by each game. Straddling both sub-rows is what
// centres it vertically — a lone glyph can only sit above or below the
// cell's midline. Keep it the half-block pair: full-row variants (hollow
// squares, quadrant art) render as tall slabs, not stones.
pub(super) const PUCK_SOLID: [&str; 2] = ["▄▄", "▀▀"];

/// One sub-row of a piece cell: the square stone when the tier is two rows
/// tall, the single `glyph` centered on the glyph row otherwise.
pub(super) fn piece_cell(art: [&'static str; 2], glyph: char, tier: CellTier, sub: u16) -> String {
    if tier.ch == 2 {
        format!("{:^1$}", art[sub as usize], tier.cw as usize)
    } else if sub == tier.glyph_sub() {
        cell_text(glyph, tier.cw)
    } else {
        " ".repeat(tier.cw as usize)
    }
}

/// One sub-row of a "playable here" marker: light rounded corner brackets
/// framing the cell on the 2-row tier, the single fallback `glyph` centered
/// on the glyph row in the cramped tier (corners need two rows to read).
pub(super) fn hint_cell(glyph: char, tier: CellTier, sub: u16) -> String {
    if tier.ch == 2 {
        let (l, r) = if sub == 0 {
            ('╭', '╮')
        } else {
            ('╰', '╯')
        };
        format!("{l}{}{r}", " ".repeat((tier.cw as usize).saturating_sub(2)))
    } else if sub == tier.glyph_sub() {
        cell_text(glyph, tier.cw)
    } else {
        " ".repeat(tier.cw as usize)
    }
}

pub(crate) fn draw(
    frame: &mut Frame,
    area: Rect,
    daily: &DailyState,
    image_protocol: Option<TerminalImageProtocol>,
    terminal_images: &mut TerminalImageFrame,
    chat: Option<crate::app::chat::ui::EmbeddedRoomChatView<'_>>,
) {
    let Some(board) = &daily.board else {
        frame.render_widget(
            Paragraph::new("No daily match open. Press Esc to go back.")
                .alignment(Alignment::Center),
            area,
        );
        return;
    };
    board.board_geometry.set(None);
    board.target_geometry.set(None);

    if let Some(error) = &board.load_error {
        draw_center_message(frame, area, &format!("Failed to load match: {error}"));
        return;
    }
    let Some(detail) = &board.detail else {
        draw_center_message(frame, area, "Loading match…");
        return;
    };

    // Match chat rides below the board like the active-room split; the board
    // area shrinks before the game picks its tier so sizing stays honest.
    let (game_area, spacer_area, chat_area) = split_board_and_chat(area, chat.is_some());
    draw_match(
        frame,
        game_area,
        daily,
        board,
        detail,
        image_protocol,
        terminal_images,
    );
    if let (Some(chat), Some(chat_area)) = (chat, chat_area) {
        if let Some(spacer_area) = spacer_area {
            draw_chat_spacer(frame, spacer_area);
        }
        crate::app::chat::ui::draw_embedded_room_chat(frame, chat_area, chat, terminal_images);
    }
}

/// `(board, spacer, chat)`: the chat slab exists only when a chat view does
/// and the terminal is tall enough to keep the board playable above it.
fn split_board_and_chat(area: Rect, has_chat: bool) -> (Rect, Option<Rect>, Option<Rect>) {
    if !has_chat || area.height < CHAT_MIN_AREA_HEIGHT {
        return (area, None, None);
    }
    let chat_h = (area.height / 3).clamp(CHAT_MIN_HEIGHT, CHAT_MAX_HEIGHT);
    let rows = Layout::vertical([
        Constraint::Min(0),
        Constraint::Length(1),
        Constraint::Length(chat_h),
    ])
    .split(area);
    (rows[0], Some(rows[1]), Some(rows[2]))
}

fn draw_chat_spacer(frame: &mut Frame, area: Rect) {
    if area.height == 0 {
        return;
    }
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "─".repeat(area.width as usize),
            Style::default().fg(theme::BORDER_DIM()),
        ))),
        area,
    );
}

fn draw_match(
    frame: &mut Frame,
    area: Rect,
    daily: &DailyState,
    board: &DailyBoardState,
    detail: &DailyMatchDetail,
    image_protocol: Option<TerminalImageProtocol>,
    terminal_images: &mut TerminalImageFrame,
) {
    if area.height < 10 || area.width < 30 {
        frame.render_widget(Paragraph::new("The board needs more room."), area);
        return;
    }

    let chess = match &detail.game {
        DailyGameDetail::Chess(chess) => chess,
        DailyGameDetail::Battleship(battleship) => {
            super::battleship_ui::draw(frame, area, daily, board, detail, battleship);
            return;
        }
        DailyGameDetail::Connect4(connect4) => {
            super::connect4_ui::draw(frame, area, daily, board, detail, connect4);
            return;
        }
        DailyGameDetail::Reversi(reversi) => {
            super::reversi_ui::draw(frame, area, daily, board, detail, reversi);
            return;
        }
        DailyGameDetail::Checkers(checkers) => {
            super::checkers_ui::draw(frame, area, daily, board, detail, checkers);
            return;
        }
        DailyGameDetail::Backgammon(backgammon) => {
            super::backgammon_ui::draw(frame, area, daily, board, detail, backgammon);
            return;
        }
    };

    let show_sidebar = area.width >= INFO_SIDEBAR_MIN_WIDTH;
    let content = if show_sidebar {
        let cols =
            Layout::horizontal([Constraint::Fill(1), Constraint::Length(INFO_SIDEBAR_WIDTH)])
                .split(area);
        draw_info_rail(frame, cols[1], chess);
        cols[0]
    } else {
        area
    };

    // Size the board to the space left after the four chrome rows (status,
    // two player bars, key hints). The status and player bars ride with the
    // board so the colour labels hug it, and the centring keeps that group
    // mid-screen. Only the key hints break away: they pin to the last row, out
    // of the way of the board, with the slack absorbed between the two.
    const CHROME_ROWS: u16 = 4;
    let tier = pick_tier(
        content.width as usize,
        content.height.saturating_sub(CHROME_ROWS) as usize,
    );
    let board_h = (tier.board_h() as u16).min(content.height.saturating_sub(CHROME_ROWS));
    let stack_h = board_h + CHROME_ROWS;
    let top_pad = content.height.saturating_sub(stack_h) / 2;

    let rows = Layout::vertical([
        Constraint::Length(top_pad),
        Constraint::Length(1),       // status
        Constraint::Length(1),       // top player bar
        Constraint::Length(board_h), // board
        Constraint::Length(1),       // bottom player bar
        Constraint::Min(0),          // slack, pushing the hints to the floor
        Constraint::Length(1),       // key hints
    ])
    .split(content);
    let (status_row, top_bar, board_row, bottom_bar, hint_row) =
        (rows[1], rows[2], rows[3], rows[4], rows[6]);

    let orientation = daily.board_orientation();
    let my_turn = detail.is_active() && detail.row.turn_user_id == Some(daily.user_id());
    let legal = daily.board_legal_targets();
    let bar_width = (tier.board_w() as u16).min(content.width);

    frame.render_widget(
        Paragraph::new(status_line(daily, board, detail, chess)).alignment(Alignment::Center),
        status_row,
    );
    draw_player_bar(
        frame,
        centered_x(top_bar, bar_width),
        board,
        detail,
        chess,
        orientation.other(),
    );

    let finished = !detail.is_active();
    // A finished match is a static win/lose board nobody can move on. Render
    // the final position in ASCII: the terminal PNG pieces go stale the moment
    // we stop re-pushing their placements (they linger as broken ghosts on
    // protocols with no delete-by-id), so the graphics path is wrong here.
    let render_mode = if finished {
        ChessPieceRenderMode::Ascii
    } else {
        board.piece_render_mode
    };
    let board_ctx = BoardCtx {
        orientation,
        cursor: my_turn.then_some(board.cursor),
        selected: board.selected,
        last: chess.state.last_move().map(|mv| (mv.from, mv.to)),
        check_sq: chess
            .in_check
            .then(|| board_ui::king_square(&chess.pieces, chess.turn))
            .flatten(),
    };
    let board_area = board_ui::draw_board(
        frame,
        board_row,
        tier,
        &chess.pieces,
        &board_ctx,
        &legal,
        board.match_id,
        image_protocol,
        terminal_images,
        render_mode,
        false,
    );
    if let Some(board_area) = board_area {
        board.board_geometry.set(Some((board_area, tier)));
    }

    draw_player_bar(
        frame,
        centered_x(bottom_bar, bar_width),
        board,
        detail,
        chess,
        orientation,
    );
    frame.render_widget(
        Paragraph::new(key_line(board, detail)).alignment(Alignment::Center),
        hint_row,
    );
}

pub(super) fn draw_center_message(frame: &mut Frame, area: Rect, message: &str) {
    let rows = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Fill(1),
    ])
    .split(area);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            message.to_string(),
            Style::default().fg(theme::TEXT_DIM()),
        )))
        .alignment(Alignment::Center),
        rows[1],
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "esc back",
            Style::default().fg(theme::TEXT_FAINT()),
        )))
        .alignment(Alignment::Center),
        rows[2],
    );
}

fn status_line(
    daily: &DailyState,
    board: &DailyBoardState,
    detail: &DailyMatchDetail,
    chess: &ChessDetail,
) -> Line<'static> {
    if board.resign_confirm {
        return Line::from(Span::styled(
            "Resign this match? Press r again to confirm.",
            Style::default()
                .fg(theme::ERROR())
                .add_modifier(Modifier::BOLD),
        ));
    }
    let deadline = detail
        .row
        .turn_deadline_at
        .map(|at| format_deadline(at, Utc::now()));
    let mut spans = Vec::new();
    if detail.is_active() {
        let my_turn = detail.row.turn_user_id == Some(daily.user_id());
        let text = if my_turn {
            "Your move".to_string()
        } else {
            format!(
                "Waiting for {}",
                name_for(board, detail.row.turn_user_id.unwrap_or(Uuid::nil()))
            )
        };
        spans.push(Span::styled(
            text,
            Style::default()
                .fg(if my_turn {
                    theme::AMBER()
                } else {
                    theme::TEXT_DIM()
                })
                .add_modifier(Modifier::BOLD),
        ));
        if let Some(deadline) = deadline {
            spans.push(Span::styled(
                format!("   {deadline} on the clock"),
                Style::default().fg(theme::TEXT_DIM()),
            ));
        }
        if chess.in_check {
            spans.push(Span::styled(
                "   check",
                Style::default()
                    .fg(theme::ERROR())
                    .add_modifier(Modifier::BOLD),
            ));
        }
    } else {
        let (heading, subtitle, color) = result_banner(daily, board, detail);
        spans.push(Span::styled(
            format!("{heading} · {subtitle}"),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ));
    }
    if let Some(mv) = chess.state.move_history.last() {
        spans.push(Span::styled(
            format!("   last {}", mv.label),
            Style::default().fg(theme::TEXT_DIM()),
        ));
    }
    Line::from(spans)
}

pub(super) fn result_banner(
    daily: &DailyState,
    board: &DailyBoardState,
    detail: &DailyMatchDetail,
) -> (&'static str, String, Color) {
    let winner_text = |winner: Option<Uuid>| -> String {
        match winner {
            Some(id) if id == daily.user_id() => "you win".to_string(),
            Some(id) => format!("{} wins", name_for(board, id)),
            None => "no winner".to_string(),
        }
    };
    let won = detail.row.winner_user_id == Some(daily.user_id());
    let color = if won {
        theme::SUCCESS()
    } else if detail.row.winner_user_id.is_some() {
        theme::AMBER()
    } else {
        theme::TEXT_MUTED()
    };
    match detail.row.result.as_str() {
        DailyMatch::RESULT_CHECKMATE => {
            ("Checkmate", winner_text(detail.row.winner_user_id), color)
        }
        DailyMatch::RESULT_DRAW => ("Draw", "game drawn".to_string(), theme::TEXT_MUTED()),
        DailyMatch::RESULT_RESIGN => ("Resignation", winner_text(detail.row.winner_user_id), color),
        DailyMatch::RESULT_FLEET_SUNK => {
            ("Fleet sunk", winner_text(detail.row.winner_user_id), color)
        }
        DailyMatch::RESULT_FOUR_IN_A_ROW => (
            "Four in a row",
            winner_text(detail.row.winner_user_id),
            color,
        ),
        DailyMatch::RESULT_MOST_DISCS => {
            ("Most discs", winner_text(detail.row.winner_user_id), color)
        }
        DailyMatch::RESULT_NO_MOVES => ("Game over", winner_text(detail.row.winner_user_id), color),
        DailyMatch::RESULT_BORNE_OFF => {
            ("Borne off", winner_text(detail.row.winner_user_id), color)
        }
        DailyMatch::RESULT_TIMEOUT => (
            "Timeout",
            format!(
                "{} on the 24h clock",
                winner_text(detail.row.winner_user_id)
            ),
            color,
        ),
        _ if detail.row.status == DailyMatch::STATUS_CANCELLED => (
            "Cancelled",
            "challenge withdrawn".to_string(),
            theme::TEXT_MUTED(),
        ),
        _ => ("Finished", winner_text(detail.row.winner_user_id), color),
    }
}

fn draw_player_bar(
    frame: &mut Frame,
    rect: Rect,
    board: &DailyBoardState,
    detail: &DailyMatchDetail,
    chess: &ChessDetail,
    color: ChessColor,
) {
    if rect.height == 0 {
        return;
    }
    let user_id = chess.state.user_for_color(color);
    let on_turn = detail.is_active() && chess.turn == color;
    let dot_color = if on_turn {
        theme::AMBER_GLOW()
    } else {
        theme::TEXT_FAINT()
    };
    let mut left = vec![
        Span::raw("  "),
        Span::styled("\u{25CF} ", Style::default().fg(dot_color)),
        Span::styled(
            format!("{} ", color.label()),
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(name_for(board, user_id), Style::default().fg(theme::TEXT())),
    ];

    // Pieces this colour has captured (its opponent's missing material), plus a
    // running material lead on whichever side is ahead.
    let captured = captured_by(&chess.pieces, color);
    if !captured.is_empty() {
        let glyphs: String = captured.iter().map(|kind| piece_glyph(*kind)).collect();
        left.push(Span::raw("   "));
        left.push(Span::styled(
            glyphs,
            Style::default().fg(theme::TEXT_FAINT()),
        ));
    }
    let advantage = material_advantage(&chess.pieces);
    let own = if color == ChessColor::White {
        advantage
    } else {
        -advantage
    };
    if own > 0 {
        left.push(Span::styled(
            format!("  +{own}"),
            Style::default()
                .fg(theme::SUCCESS())
                .add_modifier(Modifier::BOLD),
        ));
    }

    // Right-align the running deadline on the mover's bar, like a chess clock.
    let deadline = (on_turn)
        .then_some(detail.row.turn_deadline_at)
        .flatten()
        .map(|at| format_deadline(at, Utc::now()));
    let cols = Layout::horizontal([Constraint::Min(0), Constraint::Length(9)]).split(rect);
    frame.render_widget(Paragraph::new(Line::from(left)), cols[0]);
    if let Some(deadline) = deadline {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!("{deadline} "),
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            )))
            .alignment(Alignment::Right),
            cols[1],
        );
    }
}

// ── Material ───────────────────────────────────────────────────

const START_COUNTS: [(ChessPieceKind, usize); 5] = [
    (ChessPieceKind::Queen, 1),
    (ChessPieceKind::Rook, 2),
    (ChessPieceKind::Bishop, 2),
    (ChessPieceKind::Knight, 2),
    (ChessPieceKind::Pawn, 8),
];

fn count_pieces(
    pieces: &[Option<ChessPiece>; 64],
    color: ChessColor,
    kind: ChessPieceKind,
) -> usize {
    pieces
        .iter()
        .filter(|piece| matches!(piece, Some(piece) if piece.color == color && piece.kind == kind))
        .count()
}

/// Pieces the given colour has captured (its opponent's missing material),
/// heaviest first.
fn captured_by(pieces: &[Option<ChessPiece>; 64], by: ChessColor) -> Vec<ChessPieceKind> {
    let victim = by.other();
    let mut out = Vec::new();
    for (kind, start) in START_COUNTS {
        let remaining = count_pieces(pieces, victim, kind);
        for _ in remaining..start {
            out.push(kind);
        }
    }
    out
}

fn piece_value(kind: ChessPieceKind) -> i32 {
    match kind {
        ChessPieceKind::Pawn => 1,
        ChessPieceKind::Knight | ChessPieceKind::Bishop => 3,
        ChessPieceKind::Rook => 5,
        ChessPieceKind::Queen => 9,
        ChessPieceKind::King => 0,
    }
}

/// Positive when White is up material, negative when Black is.
fn material_advantage(pieces: &[Option<ChessPiece>; 64]) -> i32 {
    let white: i32 = captured_by(pieces, ChessColor::White)
        .iter()
        .map(|kind| piece_value(*kind))
        .sum();
    let black: i32 = captured_by(pieces, ChessColor::Black)
        .iter()
        .map(|kind| piece_value(*kind))
        .sum();
    white - black
}

fn key_line(board: &DailyBoardState, detail: &DailyMatchDetail) -> Line<'static> {
    let mut spans = Vec::new();
    let hint = |spans: &mut Vec<Span<'static>>, key: &str, desc: &str| {
        spans.push(Span::styled(
            key.to_string(),
            Style::default().fg(theme::AMBER()),
        ));
        spans.push(Span::styled(
            format!(" {desc}   "),
            Style::default().fg(theme::TEXT_DIM()),
        ));
    };
    if board.spectating {
        spans.push(Span::styled(
            "watching   ".to_string(),
            Style::default().fg(theme::TEXT_DIM()),
        ));
    } else if detail.is_active() {
        hint(&mut spans, "arrows/wasd", "move cursor");
        hint(&mut spans, "Space/Enter", "pick / play");
        hint(&mut spans, "r", "resign");
    }
    hint(
        &mut spans,
        "p",
        if board.piece_render_mode == ChessPieceRenderMode::Graphics {
            "pieces png"
        } else {
            "pieces ascii"
        },
    );
    if !board.spectating && detail.row.chat_room_id.is_some() {
        hint(&mut spans, "i", "chat");
    }
    hint(&mut spans, "Esc", "back to lobby");
    if let Some(last) = spans.last_mut() {
        let trimmed = last.content.trim_end().to_string();
        *last = Span::styled(trimmed, Style::default().fg(theme::TEXT_DIM()));
    }
    Line::from(spans)
}

fn draw_info_rail(frame: &mut Frame, area: Rect, chess: &ChessDetail) {
    let block = Block::default()
        .borders(Borders::LEFT)
        .border_style(Style::default().fg(theme::BORDER_DIM()));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let inner = Rect {
        x: inner.x + 1,
        y: inner.y,
        width: inner.width.saturating_sub(1),
        height: inner.height,
    };

    // The player bars and status line now carry names, deadline, turn and
    // material, so the rail is just context plus the one thing that has
    // nowhere else to live: the full move list.
    let mut lines = vec![
        Line::from(Span::styled(
            "Correspondence chess".to_string(),
            Style::default()
                .fg(theme::TEXT_DIM())
                .add_modifier(Modifier::ITALIC),
        )),
        Line::from(Span::styled(
            "one move per day".to_string(),
            Style::default()
                .fg(theme::TEXT_FAINT())
                .add_modifier(Modifier::ITALIC),
        )),
        Line::raw(""),
        Line::from(Span::styled(
            "Moves".to_string(),
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        )),
    ];

    let budget = (inner.height as usize).saturating_sub(lines.len());
    append_moves(&mut lines, chess, budget);
    frame.render_widget(Paragraph::new(lines), inner);
}

fn append_moves(lines: &mut Vec<Line<'static>>, chess: &ChessDetail, budget: usize) {
    if budget == 0 {
        return;
    }
    let history = &chess.state.move_history;
    if history.is_empty() {
        lines.push(Line::from(Span::styled(
            "no moves yet",
            Style::default()
                .fg(theme::TEXT_FAINT())
                .add_modifier(Modifier::ITALIC),
        )));
        return;
    }
    let mut pairs: Vec<Line<'static>> = Vec::new();
    let mut idx = 0;
    let mut number = 1;
    while idx < history.len() {
        let white = history[idx].label.clone();
        let black = history.get(idx + 1).map(|mv| mv.label.clone());
        let mut spans = vec![
            Span::styled(
                format!("{number:>3}. "),
                Style::default().fg(theme::TEXT_FAINT()),
            ),
            Span::styled(format!("{white:<9}"), Style::default().fg(theme::TEXT())),
        ];
        if let Some(black) = black {
            spans.push(Span::styled(black, Style::default().fg(theme::TEXT_DIM())));
        }
        pairs.push(Line::from(spans));
        idx += 2;
        number += 1;
    }
    if pairs.len() <= budget {
        lines.extend(pairs);
    } else {
        lines.push(Line::from(Span::styled(
            "  \u{22EE}",
            Style::default().fg(theme::TEXT_FAINT()),
        )));
        let skip = pairs.len() - (budget - 1);
        lines.extend(pairs.into_iter().skip(skip));
    }
}

pub(super) fn name_for(board: &DailyBoardState, user_id: Uuid) -> String {
    board
        .names
        .get(&user_id)
        .cloned()
        .unwrap_or_else(|| "player".to_string())
}

fn centered_x(rect: Rect, width: u16) -> Rect {
    let width = width.min(rect.width);
    Rect {
        x: rect.x + (rect.width - width) / 2,
        y: rect.y,
        width,
        height: rect.height,
    }
}
