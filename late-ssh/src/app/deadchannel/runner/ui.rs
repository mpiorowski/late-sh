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
/// The theme has no cyan or magenta, so those two are the city's neon,
/// fixed. Gold (earned) is absent on purpose: not a tint a look can carry
/// yet.
pub fn tint_color(tint: Tint) -> Color {
    match tint {
        Tint::Static => theme::TEXT_DIM(),
        Tint::Amber => theme::AMBER(),
        Tint::Phosphor => theme::BONSAI_LEAF(),
        Tint::Cyan => Color::Rgb(38, 217, 255),
        Tint::Magenta => Color::Rgb(255, 77, 204),
        Tint::Red => theme::ERROR(),
        Tint::White => theme::TEXT_BRIGHT(),
    }
}

/// The level badge beside a runner's name on the wire: the mark glyph
/// and the level, no space (`▚7`), so it reads as one token in the badge
/// stack. With Old Signal kills the count rides behind the Signal's own
/// `╬` (`▚7╬2`): the paragon number, which only ever goes up.
pub fn badge_text(entry: &RunnerEntry) -> String {
    match entry.marks {
        0 => format!("{}{}", entry.look.mark, entry.level),
        marks => format!("{}{}╬{marks}", entry.look.mark, entry.level),
    }
}

/// The band a level paints in: the newest tint the level has unlocked at
/// the tailor, so a badge never shows a color the portrait beside it
/// cannot wear. Grey to three (amber opens with it, the badge starts
/// grey), then phosphor, cyan, magenta, red every three levels, white at
/// the top of the ladder (fifteen, `fight::state::exp_to_advance` returns
/// nothing past it).
pub fn level_color(level: i32) -> Color {
    let tint = match level {
        i32::MIN..=3 => Tint::Static,
        4..=6 => Tint::Phosphor,
        7..=9 => Tint::Cyan,
        10..=12 => Tint::Magenta,
        13..=14 => Tint::Red,
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
