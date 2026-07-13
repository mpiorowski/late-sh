use std::time::Instant;

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use uuid::Uuid;

use crate::app::{
    common::theme,
    games::chess_core::{
        board_ui::{self, BoardCtx, Tier, pick_tier},
        types::{
            ChessColor, ChessGameResult, ChessMoveRecord, ChessPieceKind, ChessPieceRenderMode,
            piece_glyph,
        },
    },
    house::game_ui::{
        draw_game_frame_with_info_sidebar, draw_game_overlay, info_label_value, info_tagline,
        key_hint, payout_cooldown_label,
    },
    rooms::{
        backend::GameDrawCtx,
        chess::{
            state::{ChessPhase, State},
            svc::{CHESS_WIN_CHIP_PAYOUT, CHESS_WIN_PAYOUT_COOLDOWN, ChessSnapshot},
        },
    },
};
use crate::usernames::UsernameLookup;

const INFO_SIDEBAR_WIDTH: u16 = 28;
const INFO_SIDEBAR_MIN_WIDTH: u16 = 96;

/// Exact pane height the chess board wants: just enough for the largest
/// board that fits, plus the four chrome rows. Sized to content (like the
/// blackjack table) so the Info sidebar never stretches into a void.
pub fn preferred_height(area: Rect) -> u16 {
    let show_sidebar = area.width >= INFO_SIDEBAR_MIN_WIDTH;
    let content_w = if show_sidebar {
        area.width.saturating_sub(INFO_SIDEBAR_WIDTH)
    } else {
        area.width
    } as usize;
    let region = (area.height as usize).saturating_sub(chrome_rows(show_sidebar) as usize + 9);
    let tier = pick_tier(content_w, region);
    tier.board_h() as u16 + chrome_rows(show_sidebar)
}

fn chrome_rows(show_sidebar: bool) -> u16 {
    if show_sidebar {
        3 // status + two player bars; the rail carries key hints
    } else {
        4 // status + two player bars + key hints
    }
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

pub(crate) fn board_square_at(area: Rect, state: &State, x: u16, y: u16) -> Option<usize> {
    board_square_at_for_orientation(area, state.orienting_color(), x, y)
}

fn board_square_at_for_orientation(
    area: Rect,
    orientation: ChessColor,
    x: u16,
    y: u16,
) -> Option<usize> {
    let (board_area, tier) = board_geometry(area)?;
    board_ui::square_at(board_area, tier, orientation, x, y)
}

fn board_geometry(area: Rect) -> Option<(Rect, Tier)> {
    if area.height < 10 || area.width < 30 {
        return None;
    }

    let show_sidebar = area.width >= INFO_SIDEBAR_MIN_WIDTH;
    let content = if show_sidebar {
        Layout::horizontal([Constraint::Fill(1), Constraint::Length(INFO_SIDEBAR_WIDTH)])
            .split(area)[0]
    } else {
        area
    };
    let rows = if show_sidebar {
        Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(6),
            Constraint::Length(1),
        ])
        .split(content)
    } else {
        Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(6),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(content)
    };
    let board_region = rows[2];
    let tier = pick_tier(board_region.width as usize, board_region.height as usize);
    let board_w = (tier.board_w() as u16).min(board_region.width);
    let board_h = (tier.board_h() as u16).min(board_region.height);
    Some((
        Rect {
            x: board_region.x + board_region.width.saturating_sub(board_w) / 2,
            y: board_region.y + board_region.height.saturating_sub(board_h) / 2,
            width: board_w,
            height: board_h,
        },
        tier,
    ))
}

// ── Entry point ────────────────────────────────────────────────

pub fn draw_game(frame: &mut Frame, area: Rect, state: &State, ctx: GameDrawCtx<'_>) {
    if area.height < 10 || area.width < 30 {
        frame.render_widget(Paragraph::new("Chess board needs more room."), area);
        return;
    }

    let snapshot = state.snapshot();
    let show_sidebar = area.width >= INFO_SIDEBAR_MIN_WIDTH;
    let info = info_lines(
        snapshot,
        ctx.usernames,
        area.height as usize,
        state.piece_render_mode(),
    );
    let content = draw_game_frame_with_info_sidebar(frame, area, "Chess", info, show_sidebar);

    let rows = if show_sidebar {
        Layout::vertical([
            Constraint::Length(1), // status
            Constraint::Length(1), // top player bar
            Constraint::Min(6),    // board
            Constraint::Length(1), // bottom player bar
        ])
        .split(content)
    } else {
        Layout::vertical([
            Constraint::Length(1), // status
            Constraint::Length(1), // top player bar
            Constraint::Min(6),    // board
            Constraint::Length(1), // bottom player bar
            Constraint::Length(1), // key hints
        ])
        .split(content)
    };

    // One tier drives both the board and the player bars, so the bars line
    // up flush with the board's left and right edges.
    let tier = pick_tier(rows[2].width as usize, rows[2].height as usize);
    let bar_width = (tier.board_w() as u16).min(content.width);

    let orientation = state.orienting_color();
    let seated = state.seat_index().is_some();
    let cursor = seated.then(|| state.cursor());
    let legal = state.legal_targets();

    frame.render_widget(
        Paragraph::new(status_line(snapshot)).alignment(Alignment::Center),
        rows[0],
    );
    draw_player_bar(
        frame,
        centered_x(rows[1], bar_width),
        snapshot,
        ctx.usernames,
        orientation.other(),
    );
    let finished_overlay_open = snapshot.phase == ChessPhase::Finished && snapshot.result.is_some();
    let board_ctx = BoardCtx {
        orientation,
        cursor,
        selected: state.selected(),
        last: snapshot.last_move.as_ref().map(|mv| (mv.from, mv.to)),
        check_sq: snapshot
            .in_check
            .then(|| board_ui::king_square(&snapshot.pieces, snapshot.turn))
            .flatten(),
    };
    let board_area = board_ui::draw_board(
        frame,
        rows[2],
        tier,
        &snapshot.pieces,
        &board_ctx,
        &legal,
        state.room_id(),
        ctx.image_protocol,
        ctx.terminal_images,
        state.piece_render_mode(),
        finished_overlay_open,
    );
    if let (Some(board_area), Some(result)) = (
        board_area,
        finished_overlay_open.then_some(snapshot.result).flatten(),
    ) {
        let (heading, subtitle, color) = result_overlay(result);
        draw_game_overlay(frame, board_area, heading, &subtitle, color);
    }
    draw_player_bar(
        frame,
        centered_x(rows[3], bar_width),
        snapshot,
        ctx.usernames,
        orientation,
    );
    if !show_sidebar {
        frame.render_widget(
            Paragraph::new(key_line(state)).alignment(Alignment::Center),
            rows[4],
        );
    }
}

fn result_overlay(result: ChessGameResult) -> (&'static str, String, ratatui::style::Color) {
    match result {
        ChessGameResult::Checkmate { winner } => (
            "Checkmate",
            format!("{} wins", winner.label()),
            theme::SUCCESS(),
        ),
        ChessGameResult::Timeout { winner } => (
            "Flag fall",
            format!("{} wins on time", winner.label()),
            theme::AMBER(),
        ),
        ChessGameResult::Resignation { winner } => (
            "Resignation",
            format!("{} wins", winner.label()),
            theme::AMBER(),
        ),
        ChessGameResult::Draw => ("Draw", "game drawn".to_string(), theme::TEXT_MUTED()),
    }
}

// ── Player bars ────────────────────────────────────────────────

fn draw_player_bar(
    frame: &mut Frame,
    rect: Rect,
    snapshot: &ChessSnapshot,
    usernames: &UsernameLookup<'_>,
    color: ChessColor,
) {
    if rect.height == 0 {
        return;
    }
    let index = color.seat_index();
    let active = snapshot.phase == ChessPhase::Active && snapshot.turn == color;
    let seated = snapshot.seats[index].is_some();
    let name = seat_name(snapshot.seats[index], usernames);
    let (clock_str, secs) = clock_for(snapshot, index);

    let dot_color = if active {
        theme::AMBER_GLOW()
    } else {
        theme::TEXT_FAINT()
    };
    let name_color = if seated {
        theme::TEXT()
    } else {
        theme::TEXT_MUTED()
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
        Span::styled(name, Style::default().fg(name_color)),
    ];
    if seated && snapshot.phase != ChessPhase::Active && snapshot.ready[index] {
        left.push(Span::styled(
            "  ready",
            Style::default()
                .fg(theme::SUCCESS())
                .add_modifier(Modifier::BOLD),
        ));
    }

    let captured = captured_pieces(snapshot, color);
    if !captured.is_empty() {
        let glyphs: String = captured.iter().map(|kind| piece_glyph(*kind)).collect();
        left.push(Span::raw("   "));
        left.push(Span::styled(
            glyphs,
            Style::default().fg(theme::TEXT_FAINT()),
        ));
    }
    let advantage = material_advantage(snapshot);
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

    let clock_color = if active && secs.is_some_and(|secs| secs < 30) {
        theme::ERROR()
    } else if active {
        theme::AMBER()
    } else {
        theme::TEXT_BRIGHT()
    };

    let cols = Layout::horizontal([Constraint::Min(0), Constraint::Length(9)]).split(rect);
    frame.render_widget(Paragraph::new(Line::from(left)), cols[0]);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!("{clock_str} "),
            Style::default()
                .fg(clock_color)
                .add_modifier(Modifier::BOLD),
        )))
        .alignment(Alignment::Right),
        cols[1],
    );
}

// ── Material ───────────────────────────────────────────────────

const START_COUNTS: [(ChessPieceKind, usize); 5] = [
    (ChessPieceKind::Queen, 1),
    (ChessPieceKind::Rook, 2),
    (ChessPieceKind::Bishop, 2),
    (ChessPieceKind::Knight, 2),
    (ChessPieceKind::Pawn, 8),
];

fn count_pieces(snapshot: &ChessSnapshot, color: ChessColor, kind: ChessPieceKind) -> usize {
    snapshot
        .pieces
        .iter()
        .filter(|piece| matches!(piece, Some(piece) if piece.color == color && piece.kind == kind))
        .count()
}

/// Pieces the given colour has captured (its opponent's missing material).
fn captured_pieces(snapshot: &ChessSnapshot, by: ChessColor) -> Vec<ChessPieceKind> {
    let victim = by.other();
    let mut out = Vec::new();
    for (kind, start) in START_COUNTS {
        let remaining = count_pieces(snapshot, victim, kind);
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
fn material_advantage(snapshot: &ChessSnapshot) -> i32 {
    let white: i32 = captured_pieces(snapshot, ChessColor::White)
        .iter()
        .map(|kind| piece_value(*kind))
        .sum();
    let black: i32 = captured_pieces(snapshot, ChessColor::Black)
        .iter()
        .map(|kind| piece_value(*kind))
        .sum();
    white - black
}

// ── Status / keys ──────────────────────────────────────────────

fn status_line(snapshot: &ChessSnapshot) -> Line<'static> {
    let color = match snapshot.phase {
        ChessPhase::Active => theme::AMBER(),
        ChessPhase::Finished => theme::SUCCESS(),
        _ => theme::TEXT_DIM(),
    };
    let mut spans = vec![Span::styled(
        snapshot.status_message.clone(),
        Style::default().fg(color).add_modifier(Modifier::BOLD),
    )];
    if let Some(mv) = &snapshot.last_move {
        spans.push(Span::styled(
            format!("   last {}", mv.label),
            Style::default().fg(theme::TEXT_DIM()),
        ));
    }
    Line::from(spans)
}

fn key_line(state: &State) -> Line<'static> {
    let seated = state.seat_index().is_some();
    let active = state.snapshot().phase == ChessPhase::Active;
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

    if seated {
        hint(&mut spans, "arrows/wasd", "move cursor");
        hint(&mut spans, "Space/Enter", "pick / play");
        if active {
            hint(&mut spans, "r", "resign");
        } else {
            hint(&mut spans, "n", "ready / start");
            hint(&mut spans, "l", "stand up");
        }
    } else {
        hint(&mut spans, "s/Space/Enter", "take a seat");
    }
    hint(
        &mut spans,
        "p",
        if state.graphics_enabled() {
            "pieces png"
        } else {
            "pieces ascii"
        },
    );
    hint(&mut spans, "q", "leave room");

    // Drop the trailing separator padding from the final hint.
    if let Some(last) = spans.last_mut() {
        let trimmed = last.content.trim_end().to_string();
        *last = Span::styled(trimmed, Style::default().fg(theme::TEXT_DIM()));
    }
    Line::from(spans)
}

// ── Clocks ─────────────────────────────────────────────────────

fn clock_for(snapshot: &ChessSnapshot, index: usize) -> (String, Option<u64>) {
    if snapshot.phase == ChessPhase::Active
        && snapshot.turn.seat_index() == index
        && let Some(deadline) = snapshot.active_deadline
    {
        let secs = deadline.saturating_duration_since(Instant::now()).as_secs();
        return (format_duration(secs), Some(secs));
    }
    let clock = snapshot.clocks[index];
    if let Some(deadline) = clock.move_deadline {
        let secs = deadline.saturating_duration_since(Instant::now()).as_secs();
        return (format_duration(secs), Some(secs));
    }
    match clock.remaining_secs {
        Some(secs) => (format_duration(secs), Some(secs)),
        None => ("--".to_string(), None),
    }
}

fn format_duration(secs: u64) -> String {
    if secs >= 24 * 60 * 60 {
        let days = secs.div_ceil(24 * 60 * 60);
        return format!("{days}d");
    }
    let minutes = secs / 60;
    let seconds = secs % 60;
    format!("{minutes}:{seconds:02}")
}

// ── Info sidebar ───────────────────────────────────────────────

fn info_lines(
    snapshot: &ChessSnapshot,
    usernames: &UsernameLookup<'_>,
    area_height: usize,
    render_mode: ChessPieceRenderMode,
) -> Vec<Line<'static>> {
    let white = seat_name(snapshot.seats[0], usernames);
    let black = seat_name(snapshot.seats[1], usernames);
    let state = match snapshot.result {
        Some(ChessGameResult::Checkmate { winner }) => format!("{} mate", winner.label()),
        Some(ChessGameResult::Timeout { winner }) => format!("{} on time", winner.label()),
        Some(ChessGameResult::Resignation { winner }) => format!("{} resigned", winner.label()),
        Some(ChessGameResult::Draw) => "draw".to_string(),
        None => phase_label(snapshot),
    };

    let mut lines = vec![
        info_tagline("Timed chess room."),
        Line::raw(""),
        info_label_value("White", white, theme::TEXT_BRIGHT()),
        info_label_value("Black", black, theme::TEXT_BRIGHT()),
        info_label_value("Clock", snapshot.time_control_label.clone(), theme::AMBER()),
        info_label_value(
            "Prize",
            format!("{} chips", CHESS_WIN_CHIP_PAYOUT),
            theme::SUCCESS(),
        ),
        info_label_value(
            "Cooldown",
            payout_cooldown_label(CHESS_WIN_PAYOUT_COOLDOWN),
            theme::TEXT_DIM(),
        ),
        info_label_value("State", state, theme::SUCCESS()),
        Line::raw(""),
        key_hint("arrows/wasd", "move cursor"),
        key_hint("Space/Enter", "select / move"),
        key_hint("click", "select / move"),
        key_hint("n", "ready / start"),
        key_hint("l", "stand up"),
        key_hint("r", "resign active"),
        key_hint(
            "p",
            if render_mode == ChessPieceRenderMode::Graphics {
                "pieces png"
            } else {
                "pieces ascii"
            },
        ),
        key_hint("q", "leave room"),
        Line::raw(""),
        section_header("Move list"),
    ];

    let budget = area_height.saturating_sub(2 + lines.len());
    append_moves(&mut lines, &snapshot.move_history, budget);
    lines
}

fn append_moves(lines: &mut Vec<Line<'static>>, history: &[ChessMoveRecord], budget: usize) {
    if budget == 0 {
        return;
    }
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
        pairs.push(move_pair_line(number, white, black));
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

fn move_pair_line(number: usize, white: String, black: Option<String>) -> Line<'static> {
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
    Line::from(spans)
}

fn section_header(text: &str) -> Line<'static> {
    Line::from(Span::styled(
        text.to_string(),
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD),
    ))
}

fn phase_label(snapshot: &ChessSnapshot) -> String {
    match snapshot.phase {
        ChessPhase::Waiting => "waiting".to_string(),
        ChessPhase::Ready => ready_phase_label(snapshot),
        ChessPhase::Active => format!("{} to move", snapshot.turn.label()),
        ChessPhase::Finished => "finished".to_string(),
    }
}

fn ready_phase_label(snapshot: &ChessSnapshot) -> String {
    match snapshot.ready {
        [true, false] => "White ready".to_string(),
        [false, true] => "Black ready".to_string(),
        [true, true] => "starting".to_string(),
        [false, false] => "ready".to_string(),
    }
}

fn seat_name(user_id: Option<Uuid>, usernames: &UsernameLookup<'_>) -> String {
    match user_id {
        Some(id) => usernames
            .get(&id)
            .cloned()
            .unwrap_or_else(|| "player".to_string()),
        None => "open seat".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::games::chess_core::types::ChessPiece;
    use crate::app::rooms::chess::svc::ChessClockSnapshot;

    fn starting_pieces() -> [Option<ChessPiece>; 64] {
        use ChessPieceKind::{Bishop, King, Knight, Pawn, Queen, Rook};
        let back = [Rook, Knight, Bishop, Queen, King, Bishop, Knight, Rook];
        let mut pieces: [Option<ChessPiece>; 64] = [None; 64];
        for file in 0..8 {
            pieces[file] = Some(ChessPiece {
                color: ChessColor::White,
                kind: back[file],
            });
            pieces[8 + file] = Some(ChessPiece {
                color: ChessColor::White,
                kind: Pawn,
            });
            pieces[48 + file] = Some(ChessPiece {
                color: ChessColor::Black,
                kind: Pawn,
            });
            pieces[56 + file] = Some(ChessPiece {
                color: ChessColor::Black,
                kind: back[file],
            });
        }
        pieces
    }

    fn sample_snapshot() -> ChessSnapshot {
        ChessSnapshot {
            room_id: Uuid::nil(),
            seats: [None, None],
            ready: [false, false],
            pieces: starting_pieces(),
            turn: ChessColor::White,
            phase: ChessPhase::Waiting,
            result: None,
            status_message: "test".to_string(),
            legal_moves: Vec::new(),
            last_move: None,
            clocks: [ChessClockSnapshot::default(); 2],
            active_deadline: None,
            time_control_label: "rapid 15+10".to_string(),
            in_check: false,
            move_history: Vec::new(),
        }
    }

    #[test]
    fn captured_material_tracks_missing_pieces() {
        let mut snapshot = sample_snapshot();
        // Remove a black knight and a black pawn: White is up 4.
        snapshot.pieces[57] = None;
        snapshot.pieces[48] = None;
        assert_eq!(material_advantage(&snapshot), 4);
        assert_eq!(captured_pieces(&snapshot, ChessColor::White).len(), 2);
        assert!(captured_pieces(&snapshot, ChessColor::Black).is_empty());
    }

    #[test]
    fn board_square_hit_test_maps_orientation() {
        let area = Rect::new(10, 5, 80, 32);
        let (board, tier) = board_geometry(area).expect("board should fit");

        let top_left_x = board.x + tier.gutter as u16;
        let top_left_y = board.y + 1;
        assert_eq!(
            board_square_at_for_orientation(area, ChessColor::White, top_left_x, top_left_y),
            Some(56)
        );
        assert_eq!(
            board_square_at_for_orientation(area, ChessColor::Black, top_left_x, top_left_y),
            Some(7)
        );
    }

    #[test]
    fn board_square_hit_test_ignores_labels_and_gutters() {
        let area = Rect::new(0, 0, 80, 32);
        let (board, tier) = board_geometry(area).expect("board should fit");

        assert_eq!(
            board_square_at_for_orientation(area, ChessColor::White, board.x, board.y),
            None
        );
        assert_eq!(
            board_square_at_for_orientation(area, ChessColor::White, board.x, board.y + 1),
            None
        );
        assert_eq!(
            board_square_at_for_orientation(
                area,
                ChessColor::White,
                board.x + tier.gutter as u16,
                board.bottom() - 1
            ),
            None
        );
    }

    #[test]
    fn seat_name_distinguishes_open_from_unknown_occupied_seat() {
        let user_id = Uuid::from_u128(1);
        let usernames = std::collections::HashMap::new();
        let username_lookup = UsernameLookup::new(&usernames, None);

        assert_eq!(seat_name(None, &username_lookup), "open seat");
        assert_eq!(seat_name(Some(user_id), &username_lookup), "player");
    }
}
