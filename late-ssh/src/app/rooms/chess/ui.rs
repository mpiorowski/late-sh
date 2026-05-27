use std::time::Instant;

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use uuid::Uuid;

use crate::app::{
    common::theme,
    rooms::{
        chess::{
            state::{
                ChessColor, ChessGameResult, ChessMoveRecord, ChessPhase, ChessPieceKind, State,
            },
            svc::{CHESS_WIN_CHIP_PAYOUT, CHESS_WIN_PAYOUT_COOLDOWN, ChessSnapshot},
        },
        game_ui::{
            draw_game_frame_with_info_sidebar, draw_game_overlay, info_label_value, info_tagline,
            key_hint, payout_cooldown_label,
        },
    },
};
use crate::usernames::UsernameLookup;

// ── Board palette ──────────────────────────────────────────────
// Cool slate squares pulled into the 13–23% luminance band so both
// the ivory and onyx pieces clear the ~3:1 contrast floor terminals
// use for minimum-contrast remapping. Warm amber/red highlights pop
// against the cool base.
const SQ_LIGHT: Color = Color::Rgb(120, 136, 134);
const SQ_DARK: Color = Color::Rgb(88, 102, 100);
const SQ_LIGHT_LAST: Color = Color::Rgb(134, 138, 102);
const SQ_DARK_LAST: Color = Color::Rgb(98, 102, 70);
const SQ_CURSOR: Color = Color::Rgb(176, 128, 44);
const SQ_SELECTED: Color = Color::Rgb(150, 98, 30);
const SQ_CAPTURE: Color = Color::Rgb(150, 78, 52);
const SQ_CHECK: Color = Color::Rgb(146, 56, 44);

// Pieces: ASCII silhouettes on larger boards, with a one-cell fallback for
// cramped panes. Ivory for White, onyx for Black.
const PIECE_WHITE: Color = Color::Rgb(250, 246, 236);
const PIECE_BLACK: Color = Color::Rgb(26, 24, 26);
const MARKER: Color = Color::Rgb(244, 212, 122);

const INFO_SIDEBAR_WIDTH: u16 = 28;
const INFO_SIDEBAR_MIN_WIDTH: u16 = 96;

// ── Cell sizing ────────────────────────────────────────────────

#[derive(Clone, Copy)]
struct Tier {
    cw: usize,
    ch: usize,
    gutter: usize,
}

impl Tier {
    fn board_w(self) -> usize {
        self.gutter * 2 + self.cw * 8
    }

    fn board_h(self) -> usize {
        2 + self.ch * 8
    }
}

const TIERS: [Tier; 5] = [
    Tier {
        cw: 8,
        ch: 4,
        gutter: 3,
    },
    Tier {
        cw: 6,
        ch: 3,
        gutter: 3,
    },
    Tier {
        cw: 4,
        ch: 2,
        gutter: 2,
    },
    Tier {
        cw: 3,
        ch: 2,
        gutter: 2,
    },
    Tier {
        cw: 2,
        ch: 1,
        gutter: 2,
    },
];

fn pick_tier(width: usize, height: usize) -> Tier {
    TIERS
        .iter()
        .copied()
        .find(|tier| tier.board_w() <= width && tier.board_h() <= height)
        .unwrap_or(TIERS[TIERS.len() - 1])
}

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
    if x < board_area.x || x >= board_area.right() || y < board_area.y || y >= board_area.bottom() {
        return None;
    }

    let local_x = x - board_area.x;
    let local_y = y - board_area.y;
    if local_y == 0 || local_y > tier.ch as u16 * 8 {
        return None;
    }
    let cell_x = local_x.checked_sub(tier.gutter as u16)?;
    if cell_x >= tier.cw as u16 * 8 {
        return None;
    }

    let display_col = (cell_x / tier.cw as u16) as usize;
    let display_row = ((local_y - 1) / tier.ch as u16) as usize;
    let rank = match orientation {
        ChessColor::White => 7usize.saturating_sub(display_row),
        ChessColor::Black => display_row,
    };
    let file = match orientation {
        ChessColor::White => display_col,
        ChessColor::Black => 7usize.saturating_sub(display_col),
    };
    Some(rank * 8 + file)
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

pub fn draw_game(frame: &mut Frame, area: Rect, state: &State, usernames: &UsernameLookup<'_>) {
    if area.height < 10 || area.width < 30 {
        frame.render_widget(Paragraph::new("Chess board needs more room."), area);
        return;
    }

    let snapshot = state.snapshot();
    let show_sidebar = area.width >= INFO_SIDEBAR_MIN_WIDTH;
    let info = info_lines(snapshot, usernames, area.height as usize);
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
        usernames,
        orientation.other(),
    );
    draw_board(
        frame,
        rows[2],
        tier,
        snapshot,
        orientation,
        cursor,
        state.selected(),
        &legal,
    );
    draw_player_bar(
        frame,
        centered_x(rows[3], bar_width),
        snapshot,
        usernames,
        orientation,
    );
    if !show_sidebar {
        frame.render_widget(
            Paragraph::new(key_line(state)).alignment(Alignment::Center),
            rows[4],
        );
    }
}

// ── Board ──────────────────────────────────────────────────────

struct BoardCtx {
    orientation: ChessColor,
    cursor: Option<usize>,
    selected: Option<usize>,
    last: Option<(usize, usize)>,
    check_sq: Option<usize>,
}

#[allow(clippy::too_many_arguments)]
fn draw_board(
    frame: &mut Frame,
    area: Rect,
    tier: Tier,
    snapshot: &ChessSnapshot,
    orientation: ChessColor,
    cursor: Option<usize>,
    selected: Option<usize>,
    legal: &[usize],
) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    let ctx = BoardCtx {
        orientation,
        cursor,
        selected,
        last: snapshot.last_move.as_ref().map(|mv| (mv.from, mv.to)),
        check_sq: snapshot
            .in_check
            .then(|| king_square(snapshot, snapshot.turn))
            .flatten(),
    };

    let lines = board_lines(snapshot, tier, &ctx, legal);
    let board_w = (tier.board_w() as u16).min(area.width);
    let board_h = (tier.board_h() as u16).min(area.height);
    let board_area = Rect {
        x: area.x + area.width.saturating_sub(board_w) / 2,
        y: area.y + area.height.saturating_sub(board_h) / 2,
        width: board_w,
        height: board_h,
    };
    frame.render_widget(Paragraph::new(lines), board_area);

    if snapshot.phase == ChessPhase::Finished
        && let Some(result) = snapshot.result
    {
        let (heading, subtitle, color) = result_overlay(result);
        draw_game_overlay(frame, board_area, heading, &subtitle, color);
    }
}

fn board_lines(
    snapshot: &ChessSnapshot,
    tier: Tier,
    ctx: &BoardCtx,
    legal: &[usize],
) -> Vec<Line<'static>> {
    let mut lines = Vec::with_capacity(tier.ch * 8 + 2);
    lines.push(file_label_line(ctx.orientation, tier));

    for display_row in 0..8 {
        let rank = match ctx.orientation {
            ChessColor::White => 7 - display_row,
            ChessColor::Black => display_row,
        };
        for sub in 0..tier.ch {
            let mut spans = Vec::with_capacity(tier.cw * 8 / 2 + 2);
            let label = (sub == tier.ch / 2).then_some(rank + 1);
            spans.push(gutter_span(tier.gutter, label));
            for display_col in 0..8 {
                let file = match ctx.orientation {
                    ChessColor::White => display_col,
                    ChessColor::Black => 7 - display_col,
                };
                let index = rank * 8 + file;
                push_cell_spans(&mut spans, index, sub, tier, ctx, snapshot, legal);
            }
            spans.push(gutter_span(tier.gutter, label));
            lines.push(Line::from(spans));
        }
    }

    lines.push(file_label_line(ctx.orientation, tier));
    lines
}

fn push_cell_spans(
    spans: &mut Vec<Span<'static>>,
    index: usize,
    sub: usize,
    tier: Tier,
    ctx: &BoardCtx,
    snapshot: &ChessSnapshot,
    legal: &[usize],
) {
    let piece = snapshot.pieces[index];
    let bg = square_bg(index, ctx, legal, piece.is_some());
    let cw = tier.cw;
    let bg_style = Style::default().bg(bg);

    let (cell, fg) = match piece {
        Some(piece) => {
            let fg = match piece.color {
                ChessColor::White => PIECE_WHITE,
                ChessColor::Black => PIECE_BLACK,
            };
            (piece_cell_line(piece.kind, tier, sub), fg)
        }
        None if legal.contains(&index) => (marker_cell_line(tier, sub), MARKER),
        None => (" ".repeat(cw), MARKER),
    };

    if piece.is_none() && !legal.contains(&index) {
        spans.push(Span::styled(cell, bg_style));
    } else {
        spans.push(Span::styled(
            cell,
            Style::default().bg(bg).fg(fg).add_modifier(Modifier::BOLD),
        ));
    }
}

fn piece_cell_line(kind: ChessPieceKind, tier: Tier, sub: usize) -> String {
    let Some(art) = piece_art(kind, tier) else {
        return centered_cell(&piece_glyph(kind).to_string(), tier.cw);
    };
    let glyph_h = art.len();
    let pad_top = tier.ch.saturating_sub(glyph_h) / 2;
    if sub < pad_top || sub >= pad_top + glyph_h {
        return " ".repeat(tier.cw);
    }
    centered_cell(art[sub - pad_top], tier.cw)
}

fn marker_cell_line(tier: Tier, sub: usize) -> String {
    if sub == tier.ch / 2 {
        centered_cell("*", tier.cw)
    } else {
        " ".repeat(tier.cw)
    }
}

fn centered_cell(text: &str, width: usize) -> String {
    let text_w = text.chars().count();
    if text_w >= width {
        return text.chars().take(width).collect();
    }
    let left = (width - text_w) / 2;
    let right = width - text_w - left;
    format!("{}{}{}", " ".repeat(left), text, " ".repeat(right))
}

fn piece_art(kind: ChessPieceKind, tier: Tier) -> Option<&'static [&'static str]> {
    if tier.cw >= 7 && tier.ch >= 4 {
        return Some(match kind {
            ChessPieceKind::King => KING_LARGE,
            ChessPieceKind::Queen => QUEEN_LARGE,
            ChessPieceKind::Rook => ROOK_LARGE,
            ChessPieceKind::Bishop => BISHOP_LARGE,
            ChessPieceKind::Knight => KNIGHT_LARGE,
            ChessPieceKind::Pawn => PAWN_LARGE,
        });
    }
    if tier.cw >= 5 && tier.ch >= 3 {
        return Some(match kind {
            ChessPieceKind::King => KING_MEDIUM,
            ChessPieceKind::Queen => QUEEN_MEDIUM,
            ChessPieceKind::Rook => ROOK_MEDIUM,
            ChessPieceKind::Bishop => BISHOP_MEDIUM,
            ChessPieceKind::Knight => KNIGHT_MEDIUM,
            ChessPieceKind::Pawn => PAWN_MEDIUM,
        });
    }
    if tier.cw >= 3 && tier.ch >= 2 {
        return Some(match kind {
            ChessPieceKind::King => KING_SMALL,
            ChessPieceKind::Queen => QUEEN_SMALL,
            ChessPieceKind::Rook => ROOK_SMALL,
            ChessPieceKind::Bishop => BISHOP_SMALL,
            ChessPieceKind::Knight => KNIGHT_SMALL,
            ChessPieceKind::Pawn => PAWN_SMALL,
        });
    }
    None
}

const KING_LARGE: &[&str] = &["  _+_  ", " (___) ", "  |K|  ", " /___\\ "];
const QUEEN_LARGE: &[&str] = &[" \\^^^/ ", " (___) ", "  |Q|  ", " /___\\ "];
const ROOK_LARGE: &[&str] = &[" |_|_| ", " |___| ", "  |R|  ", " /___\\ "];
const BISHOP_LARGE: &[&str] = &["  /B\\  ", " (   ) ", "  | |  ", " /___\\ "];
const KNIGHT_LARGE: &[&str] = &["  /\\_  ", " /N  ) ", "  > /  ", " /___\\ "];
const PAWN_LARGE: &[&str] = &["  ___  ", " ( P ) ", "  | |  ", " /___\\ "];

const KING_MEDIUM: &[&str] = &[" _+_ ", "( K )", "/___\\"];
const QUEEN_MEDIUM: &[&str] = &["\\^^^/", "( Q )", "/___\\"];
const ROOK_MEDIUM: &[&str] = &["|_|_|", "| R |", "/___\\"];
const BISHOP_MEDIUM: &[&str] = &[" /B\\ ", " | | ", "/___\\"];
const KNIGHT_MEDIUM: &[&str] = &[" /\\_ ", " N ) ", "/___\\"];
const PAWN_MEDIUM: &[&str] = &["  o  ", " (P) ", "/___\\"];

const KING_SMALL: &[&str] = &[" + ", "/K\\"];
const QUEEN_SMALL: &[&str] = &["^^^", "\\Q/"];
const ROOK_SMALL: &[&str] = &["|_|", "/R\\"];
const BISHOP_SMALL: &[&str] = &["/B\\", " | "];
const KNIGHT_SMALL: &[&str] = &["/N>", "/_\\"];
const PAWN_SMALL: &[&str] = &[" o ", "/P\\"];

fn piece_glyph(kind: ChessPieceKind) -> char {
    match kind {
        ChessPieceKind::Pawn => 'P',
        ChessPieceKind::Knight => 'N',
        ChessPieceKind::Bishop => 'B',
        ChessPieceKind::Rook => 'R',
        ChessPieceKind::Queen => 'Q',
        ChessPieceKind::King => 'K',
    }
}

/// Resolve a square's background colour, layering highlights by
/// priority: cursor > selected > capture > check > last move.
fn square_bg(index: usize, ctx: &BoardCtx, legal: &[usize], has_piece: bool) -> Color {
    let dark = (index / 8 + index % 8).is_multiple_of(2);
    let mut bg = if dark { SQ_DARK } else { SQ_LIGHT };

    if let Some((from, to)) = ctx.last
        && (index == from || index == to)
    {
        bg = if dark { SQ_DARK_LAST } else { SQ_LIGHT_LAST };
    }
    if ctx.check_sq == Some(index) {
        bg = SQ_CHECK;
    }
    if has_piece && legal.contains(&index) {
        bg = SQ_CAPTURE;
    }
    if ctx.selected == Some(index) {
        bg = SQ_SELECTED;
    }
    if ctx.cursor == Some(index) {
        bg = SQ_CURSOR;
    }
    bg
}

fn gutter_span(width: usize, label: Option<usize>) -> Span<'static> {
    let text = match label {
        Some(rank) => format!("{rank:^width$}"),
        None => " ".repeat(width),
    };
    Span::styled(text, Style::default().fg(theme::TEXT_DIM()))
}

fn file_label_line(orientation: ChessColor, tier: Tier) -> Line<'static> {
    let mut spans = vec![Span::raw(" ".repeat(tier.gutter))];
    for display_col in 0..8 {
        let file = match orientation {
            ChessColor::White => display_col,
            ChessColor::Black => 7 - display_col,
        };
        let label = (b'a' + file as u8) as char;
        let cw = tier.cw;
        spans.push(Span::styled(
            format!("{label:^cw$}"),
            Style::default().fg(theme::TEXT_DIM()),
        ));
    }
    spans.push(Span::raw(" ".repeat(tier.gutter)));
    Line::from(spans)
}

fn king_square(snapshot: &ChessSnapshot, color: ChessColor) -> Option<usize> {
    snapshot.pieces.iter().position(|piece| {
        matches!(piece, Some(piece) if piece.color == color && piece.kind == ChessPieceKind::King)
    })
}

fn result_overlay(result: ChessGameResult) -> (&'static str, String, Color) {
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
    use crate::app::rooms::chess::svc::{ChessClockSnapshot, ChessPiece};

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
    fn board_lines_keep_uniform_width_across_tiers() {
        let snapshot = sample_snapshot();
        for tier in TIERS {
            let ctx = BoardCtx {
                orientation: ChessColor::White,
                cursor: Some(12),
                selected: Some(8),
                last: Some((52, 36)),
                check_sq: None,
            };
            let lines = board_lines(&snapshot, tier, &ctx, &[36, 28]);
            assert_eq!(lines.len(), tier.ch * 8 + 2, "row count for cw={}", tier.cw);
            for line in &lines {
                let width: usize = line
                    .spans
                    .iter()
                    .map(|span| span.content.chars().count())
                    .sum();
                assert_eq!(width, tier.board_w(), "line width for cw={}", tier.cw);
            }
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
