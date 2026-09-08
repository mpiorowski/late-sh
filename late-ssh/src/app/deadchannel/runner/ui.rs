//! Portrait rendering: a pure function of the look. Three styled rows of
//! five cells, one per worn piece, each row in its piece's tint. Signal
//! corruption (GAME.md, "Corruption is a render effect") lands here once
//! the runner row carries signal; today every runner paints whole.

use ratatui::style::{Color, Style};
use ratatui::text::Span;

use super::state::{Look, PORTRAIT_HEIGHT, Tint};
use crate::app::common::theme;

/// The color a tint paints with, from the theme so it follows the palette.
/// Gold (earned) is absent on purpose: not a tint a look can carry yet.
pub fn tint_color(tint: Tint) -> Color {
    match tint {
        Tint::Static => theme::TEXT_DIM(),
        Tint::Amber => theme::AMBER(),
        Tint::Phosphor => theme::BONSAI_LEAF(),
        Tint::White => theme::TEXT_BRIGHT(),
        Tint::Red => theme::ERROR(),
    }
}

/// The portrait as one styled span per row, top to bottom. Each span is
/// exactly `PORTRAIT_WIDTH` cells; callers place it, never reshape it.
pub fn portrait_spans(look: &Look) -> [Span<'static>; PORTRAIT_HEIGHT] {
    look.rows()
        .map(|worn| Span::styled(worn.piece.row, Style::default().fg(tint_color(worn.tint))))
}

#[cfg(test)]
#[path = "ui_test.rs"]
mod ui_test;
