//! Shared chess-board renderer: tier sizing, square/piece drawing, terminal
//! image piece graphics, and the board-local mouse hit test. Deliberately
//! room-agnostic: callers hand in a plain piece array plus display context,
//! never a table snapshot. Surface chrome (player bars, clocks, overlays,
//! info rails) belongs to the calling domain.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use uuid::Uuid;

use crate::app::{
    common::theme,
    files::terminal_image::{TerminalImageFrame, TerminalImagePlacement, TerminalImageProtocol},
    games::chess_core::{
        piece_art::{self, GraphicsTier},
        types::{ChessColor, ChessPiece, ChessPieceKind, ChessPieceRenderMode, piece_glyph},
    },
};

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

// ── Cell sizing ────────────────────────────────────────────────

#[derive(Clone, Copy)]
pub struct Tier {
    pub cw: usize,
    pub ch: usize,
    pub gutter: usize,
}

impl Tier {
    pub fn board_w(self) -> usize {
        self.gutter * 2 + self.cw * 8
    }

    pub fn board_h(self) -> usize {
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

pub fn pick_tier(width: usize, height: usize) -> Tier {
    TIERS
        .iter()
        .copied()
        .find(|tier| tier.board_w() <= width && tier.board_h() <= height)
        .unwrap_or(TIERS[TIERS.len() - 1])
}

/// Map a click inside an already-resolved board rect to a board index,
/// honoring orientation. `x`/`y` are absolute frame coordinates.
pub fn square_at(
    board_area: Rect,
    tier: Tier,
    orientation: ChessColor,
    x: u16,
    y: u16,
) -> Option<usize> {
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

// ── Board ──────────────────────────────────────────────────────

pub struct BoardCtx {
    pub orientation: ChessColor,
    pub cursor: Option<usize>,
    pub selected: Option<usize>,
    pub last: Option<(usize, usize)>,
    pub check_sq: Option<usize>,
}

/// Draw the board centered inside `area` and return the rect it actually
/// occupied (for callers that layer overlays on top). `suppress_graphics`
/// keeps piece cells blank without pushing image placements, for frames
/// where an overlay owns the board region.
#[allow(clippy::too_many_arguments)]
pub fn draw_board(
    frame: &mut Frame,
    area: Rect,
    tier: Tier,
    pieces: &[Option<ChessPiece>; 64],
    ctx: &BoardCtx,
    legal: &[usize],
    placement_seed: Uuid,
    image_protocol: Option<TerminalImageProtocol>,
    terminal_images: &mut TerminalImageFrame,
    render_mode: ChessPieceRenderMode,
    suppress_graphics: bool,
) -> Option<Rect> {
    if area.height == 0 || area.width == 0 {
        return None;
    }

    let board_w = (tier.board_w() as u16).min(area.width);
    let board_h = (tier.board_h() as u16).min(area.height);
    let board_area = Rect {
        x: area.x + area.width.saturating_sub(board_w) / 2,
        y: area.y + area.height.saturating_sub(board_h) / 2,
        width: board_w,
        height: board_h,
    };

    let graphics_squares = if render_mode == ChessPieceRenderMode::Graphics {
        if suppress_graphics {
            occupied_piece_mask(pieces)
        } else {
            schedule_piece_graphics(
                terminal_images,
                board_area,
                tier,
                pieces,
                ctx.orientation,
                image_protocol,
                placement_seed,
            )
        }
    } else {
        0
    };

    let lines = board_lines(pieces, tier, ctx, legal, graphics_squares);
    frame.render_widget(Paragraph::new(lines), board_area);
    Some(board_area)
}

fn graphics_tier_for(tier: Tier) -> Option<GraphicsTier> {
    // Thresholds match piece_art canvas sizes exactly so a future tier
    // with cw=7 or cw=5 can't pick an image larger than its cell rect.
    if tier.cw >= 8 && tier.ch >= 4 {
        Some(GraphicsTier::Large)
    } else if tier.cw >= 6 && tier.ch >= 3 {
        Some(GraphicsTier::Medium)
    } else {
        None
    }
}

/// Push image placements for every occupied square the active protocol can
/// render, and return a 64-bit mask of which board indices the text path
/// should leave blank (so the cells beneath an image carry only the
/// background, not an ASCII glyph).
#[allow(clippy::too_many_arguments)]
fn schedule_piece_graphics(
    terminal_images: &mut TerminalImageFrame,
    board_area: Rect,
    tier: Tier,
    pieces: &[Option<ChessPiece>; 64],
    orientation: ChessColor,
    image_protocol: Option<TerminalImageProtocol>,
    placement_seed: Uuid,
) -> u64 {
    let (Some(protocol), Some(graphics_tier)) = (image_protocol, graphics_tier_for(tier)) else {
        return 0;
    };

    let mut mask = 0u64;
    for display_row in 0..8u16 {
        for display_col in 0..8u16 {
            let rank = match orientation {
                ChessColor::White => 7 - display_row,
                ChessColor::Black => display_row,
            };
            let file = match orientation {
                ChessColor::White => display_col,
                ChessColor::Black => 7 - display_col,
            };
            let index = (rank * 8 + file) as usize;
            let Some(piece) = pieces[index] else {
                continue;
            };
            let data = piece_art::graphics_image(piece.color, piece.kind, graphics_tier);
            if !data.supports_protocol(protocol) {
                continue;
            }
            let cell_rect = Rect {
                x: board_area.x + tier.gutter as u16 + display_col * tier.cw as u16,
                y: board_area.y + 1 + display_row * tier.ch as u16,
                width: tier.cw as u16,
                height: tier.ch as u16,
            };
            terminal_images.push(TerminalImagePlacement {
                message_id: piece_placement_id(placement_seed, index),
                area: cell_rect,
                data: data.clone(),
            });
            mask |= 1u64 << index;
        }
    }
    mask
}

fn piece_placement_id(placement_seed: Uuid, index: usize) -> Uuid {
    let mut bytes = *placement_seed.as_bytes();
    bytes[15] ^= index as u8;
    Uuid::from_bytes(bytes)
}

fn occupied_piece_mask(pieces: &[Option<ChessPiece>; 64]) -> u64 {
    pieces
        .iter()
        .enumerate()
        .fold(0u64, |mask, (index, piece)| {
            if piece.is_some() {
                mask | (1u64 << index)
            } else {
                mask
            }
        })
}

fn board_lines(
    pieces: &[Option<ChessPiece>; 64],
    tier: Tier,
    ctx: &BoardCtx,
    legal: &[usize],
    graphics_mask: u64,
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
                push_cell_spans(
                    &mut spans,
                    index,
                    sub,
                    tier,
                    ctx,
                    pieces,
                    legal,
                    graphics_mask,
                );
            }
            spans.push(gutter_span(tier.gutter, label));
            lines.push(Line::from(spans));
        }
    }

    lines.push(file_label_line(ctx.orientation, tier));
    lines
}

#[allow(clippy::too_many_arguments)]
fn push_cell_spans(
    spans: &mut Vec<Span<'static>>,
    index: usize,
    sub: usize,
    tier: Tier,
    ctx: &BoardCtx,
    pieces: &[Option<ChessPiece>; 64],
    legal: &[usize],
    graphics_mask: u64,
) {
    let piece = pieces[index];
    let bg = square_bg(index, ctx, legal, piece.is_some());
    let cw = tier.cw;
    let bg_style = Style::default().bg(bg);

    if graphics_mask & (1u64 << index) != 0 {
        // Graphics overlay paints the piece; cells underneath stay blank so
        // the image's transparent border lets the square bg show through
        // without an ASCII glyph competing for the same row.
        spans.push(Span::styled(" ".repeat(cw), bg_style));
        return;
    }

    match piece {
        Some(piece) => {
            push_piece_spans(spans, piece.color, piece.kind, tier, sub, bg);
        }
        None if legal.contains(&index) => {
            spans.push(Span::styled(
                marker_cell_line(tier, sub),
                Style::default()
                    .bg(bg)
                    .fg(MARKER)
                    .add_modifier(Modifier::BOLD),
            ));
        }
        None => {
            spans.push(Span::styled(" ".repeat(cw), bg_style));
        }
    }
}

fn push_piece_spans(
    spans: &mut Vec<Span<'static>>,
    color: ChessColor,
    kind: ChessPieceKind,
    tier: Tier,
    sub: usize,
    bg: Color,
) {
    let theme_fg = match color {
        ChessColor::White => PIECE_WHITE,
        ChessColor::Black => PIECE_BLACK,
    };
    let cell = ascii_piece_line(kind, tier, sub);
    spans.push(Span::styled(
        cell,
        Style::default()
            .bg(bg)
            .fg(theme_fg)
            .add_modifier(Modifier::BOLD),
    ));
}

fn ascii_piece_line(kind: ChessPieceKind, tier: Tier, sub: usize) -> String {
    let Some(art) = ascii_piece_art(kind, tier) else {
        return small_tier_letter(kind, tier, sub);
    };
    let glyph_h = art.len();
    let pad_top = tier.ch.saturating_sub(glyph_h) / 2;
    if sub < pad_top || sub >= pad_top + glyph_h {
        return " ".repeat(tier.cw);
    }
    centered_cell(art[sub - pad_top], tier.cw)
}

fn small_tier_letter(kind: ChessPieceKind, tier: Tier, sub: usize) -> String {
    if sub == tier.ch / 2 {
        centered_cell(&piece_glyph(kind).to_string(), tier.cw)
    } else {
        " ".repeat(tier.cw)
    }
}

fn ascii_piece_art(kind: ChessPieceKind, tier: Tier) -> Option<&'static [&'static str]> {
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

pub fn king_square(pieces: &[Option<ChessPiece>; 64], color: ChessColor) -> Option<usize> {
    pieces.iter().position(|piece| {
        matches!(piece, Some(piece) if piece.color == color && piece.kind == ChessPieceKind::King)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn board_lines_keep_uniform_width_across_tiers() {
        let pieces = starting_pieces();
        for tier in TIERS {
            let ctx = BoardCtx {
                orientation: ChessColor::White,
                cursor: Some(12),
                selected: Some(8),
                last: Some((52, 36)),
                check_sq: None,
            };
            let lines = board_lines(&pieces, tier, &ctx, &[36, 28], 0);
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
    fn king_square_finds_each_color() {
        let pieces = starting_pieces();
        assert_eq!(king_square(&pieces, ChessColor::White), Some(4));
        assert_eq!(king_square(&pieces, ChessColor::Black), Some(60));
    }
}
