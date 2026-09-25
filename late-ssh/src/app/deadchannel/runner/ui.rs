//! Portrait and badge rendering: pure functions of the directory entry.
//! The portrait is three styled rows of five cells, one per worn piece,
//! each row in its piece's tint. Signal corruption (GAME.md, "Corruption
//! is a render effect") lands here once the runner row carries signal;
//! today every runner paints whole. The badge is the mark and the level
//! (`▚7`), painted in the level's band color beside the name on the wire.

use ratatui::style::{Color, Style};
use ratatui::text::Span;

use super::state::{Look, PORTRAIT_HEIGHT, Tint};
use super::svc::RunnerEntry;
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

/// The level badge beside a runner's name on the wire: the mark glyph
/// and the level, no space (`▚7`), so it reads as one token in the badge
/// stack.
pub fn badge_text(entry: &RunnerEntry) -> String {
    format!("{}{}", entry.look.mark, entry.level)
}

/// The band a level paints in: grey through the first four, amber to
/// nine, phosphor to fourteen, white at the top of the ladder (fifteen,
/// `fight::state::exp_to_advance` returns nothing past it). The bands are
/// the tints a look can wear, so a badge never introduces a color the
/// portrait beside it cannot.
pub fn level_color(level: i32) -> Color {
    let tint = match level {
        i32::MIN..=4 => Tint::Static,
        5..=9 => Tint::Amber,
        10..=14 => Tint::Phosphor,
        15..=i32::MAX => Tint::White,
    };
    tint_color(tint)
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
