//! How a hung piece draws in chat: its card's rows in #lounge
//! (`lounge_card_lines`, in place of the message's line) and the piece
//! itself over the chat once Enter opens it (`draw_modal`).

use dartboard_core::{CellValue, Pos};
use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use super::state::PieceModal;
use crate::app::artboard::gallery::svc::{GalleryPiece, applause_label};
use crate::app::artboard::gallery::ui::{
    caption_line, credits_line, draw_piece_canvas, key_hint_line, notice_line,
};
use crate::app::common::primitives::{clip_spans, truncate_cells};
use crate::app::common::theme;

/// How many rows of a piece its card shows. A taller piece is cut here
/// and Enter shows the rest.
const CARD_PREVIEW_ROWS: usize = 12;
/// A chat narrower than this shows the card without a preview.
const CARD_PREVIEW_MIN_WIDTH: usize = 20;
/// The modal is never narrower than its key row needs.
const MODAL_MIN_WIDTH: u16 = 56;
/// The modal's left and right border.
const MODAL_BORDER_COLS: u16 = 2;
/// The modal's rows that are not the piece: the border and the caption,
/// credits, notice and key rows.
const MODAL_CHROME_ROWS: u16 = 6;
/// The modal's columns that are not the piece: the border and the margin
/// `draw_piece_canvas` keeps each side.
const MODAL_CHROME_COLS: u16 = 4;

/// A piece's card in `width` columns: its title, the piece itself (its top
/// left corner, when it is larger than the card, which the size then
/// says), then its size, hands and applause, and where to hang one's own. With `gated` (the viewer
/// still has the Artboard content disclaimer on) the piece is left out
/// and the row says where to look.
pub(crate) fn lounge_card_lines(
    piece: &GalleryPiece,
    width: usize,
    gated: bool,
) -> Vec<Line<'static>> {
    let faint = Style::default().fg(theme::TEXT_FAINT());
    let dim = Style::default().fg(theme::TEXT_DIM());
    let mut rows = vec![Line::from(Span::styled(
        truncate_cells(&format!("🎨 \"{}\"", piece.title.trim()), width),
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD),
    ))];

    if gated {
        rows.push(Line::from(Span::styled(
            truncate_cells(
                "Artboard content disclaimer is on · Enter to view on the Artboard",
                width,
            ),
            dim,
        )));
    } else if width >= CARD_PREVIEW_MIN_WIDTH {
        rows.extend(preview_lines(piece, width));
    }
    // The size says so when the preview is not all of the piece.
    let cut = !gated
        && width >= CARD_PREVIEW_MIN_WIDTH
        && (piece.width > width || piece.height > CARD_PREVIEW_ROWS);
    let size = match cut {
        true => format!("top left of {}x{}", piece.width, piece.height),
        false => format!("{}x{}", piece.width, piece.height),
    };

    let total: usize = piece
        .credits
        .iter()
        .map(|credit| credit.glyphs)
        .sum::<usize>()
        .max(1);
    let others: Vec<String> = piece
        .credits
        .iter()
        .filter(|credit| credit.username != piece.username)
        .take(4)
        .map(|credit| format!("@{} {}%", credit.username, credit.glyphs * 100 / total))
        .collect();
    let hands = match others.is_empty() {
        true => "all their own work".to_string(),
        false => format!("with {}", others.join(", ")),
    };
    rows.push(Line::from(clip_spans(
        vec![
            Span::styled(size, faint),
            Span::styled(" · ", faint),
            Span::styled(hands, dim),
            Span::styled(" · ", faint),
            Span::styled(applause_label(piece.applause), dim),
            Span::styled(" · Enter to view", faint),
        ],
        width,
    )));
    rows.push(Line::from(clip_spans(
        vec![
            Span::styled("hang a piece on Artboard (page 4) · ", faint),
            Span::styled(
                "monthly prizes up to 40,000 chips",
                Style::default().fg(theme::AMBER_DIM()),
            ),
        ],
        width,
    )));
    rows
}

/// The piece's top left corner in its own cells and colours, at most
/// `width` columns by `CARD_PREVIEW_ROWS` rows.
fn preview_lines(piece: &GalleryPiece, width: usize) -> Vec<Line<'static>> {
    let shown_width = piece.width.min(width);
    let shown_height = piece.height.min(CARD_PREVIEW_ROWS);
    let mut lines = Vec::with_capacity(shown_height);
    for y in 0..shown_height {
        let mut spans = Vec::new();
        let mut x = 0;
        while x < shown_width {
            let pos = Pos { x, y };
            // A wide glyph that would cross the cut is left out, as it
            // would be drawn past the card's edge.
            let (glyph, columns) = match piece.canvas.cell(pos) {
                Some(CellValue::Narrow(glyph)) => (glyph, 1),
                Some(CellValue::Wide(glyph)) if x + 1 < shown_width => (glyph, 2),
                Some(CellValue::Wide(_) | CellValue::WideCont) | None => (' ', 1),
            };
            let ink = match piece.canvas.fg(pos) {
                Some(color) => Color::Rgb(color.r, color.g, color.b),
                None => theme::TEXT(),
            };
            spans.push(Span::styled(glyph.to_string(), Style::default().fg(ink)));
            x += columns;
        }
        lines.push(Line::from(spans));
    }
    lines
}

/// The open piece over whatever screen is up: caption, hands, the notice
/// the last key left, the piece in its own cells, and the keys.
pub(crate) fn draw_modal(frame: &mut Frame, area: Rect, modal: &PieceModal) {
    let piece = modal.piece();
    let caption = caption_line(piece);
    // Wide enough for the piece and for its caption, whichever is wider.
    let width = (piece.width as u16)
        .saturating_add(MODAL_CHROME_COLS)
        .max((caption.width() as u16).saturating_add(MODAL_BORDER_COLS))
        .max(MODAL_MIN_WIDTH);
    let height = (piece.height as u16).saturating_add(MODAL_CHROME_ROWS);
    let popup = centered_rect(width, height, area);
    frame.render_widget(Clear, popup);
    let block = Block::default()
        .title(" Artboard piece ")
        .title_style(
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()))
        .style(Style::default().bg(theme::BG_CANVAS()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .split(inner);
    frame.render_widget(Paragraph::new(caption), rows[0]);
    frame.render_widget(
        Paragraph::new(credits_line(
            &piece.credits,
            &piece.username,
            piece.period_month,
        )),
        rows[1],
    );
    frame.render_widget(Paragraph::new(notice_line(modal.notice())), rows[2]);
    draw_piece_canvas(frame, rows[3], piece);
    let keys: &[(&str, &str)] = match modal.mine() {
        true => &[("o", "open in gallery"), ("Esc", "close")],
        false => &[("v", "applaud"), ("o", "open in gallery"), ("Esc", "close")],
    };
    frame.render_widget(Paragraph::new(key_hint_line(keys)), rows[4]);
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([Constraint::Length(height.min(area.height))])
        .flex(Flex::Center)
        .split(area);
    let horizontal = Layout::horizontal([Constraint::Length(width.min(area.width))])
        .flex(Flex::Center)
        .split(vertical[0]);
    horizontal[0]
}

#[cfg(test)]
#[path = "ui_test.rs"]
mod ui_test;
