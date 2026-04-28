//! In-memory easter-egg "gem" rendered at the bottom of the Special tab.
//!
//! Per-session only — no persistence. Color rolls happen on every interaction;
//! teleports (33%) brand the small gem with a count; the 10th teleport evolves
//! it into the grand gem. Grand gem clicks roll a 10% shine in a separate
//! random color.

use std::cell::Cell;

use rand_core::{OsRng, RngCore};
use ratatui::layout::Rect;
use ratatui::style::Color;

const PALETTE: &[Color] = &[
    Color::Red,
    Color::Yellow,
    Color::Green,
    Color::Cyan,
    Color::Blue,
    Color::Magenta,
    Color::LightRed,
    Color::LightYellow,
    Color::LightGreen,
    Color::LightCyan,
    Color::LightBlue,
    Color::LightMagenta,
    Color::White,
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GemPosition {
    Left,
    Right,
}

/// Direction the gem just moved during a teleport. Used to render a
/// transient speed trail on the next render. Cleared on the next
/// non-teleporting interaction so the gem only "smokes" right after a jump.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MoveDirection {
    Leftward,
    Rightward,
}

/// Keys that interact with the gem. Used to dedupe consecutive presses of
/// the same key — `<space><space>` is one interaction, `<space><j><space>`
/// is three.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GemKey {
    H,
    J,
    K,
    L,
    Space,
    Up,
    Down,
}

pub struct GemState {
    color: Color,
    position: GemPosition,
    /// Brand displayed inside the small gem. `0` = unbranded; otherwise 1..=9.
    brand: u8,
    evolved: bool,
    shining: bool,
    shine_color: Color,
    last_key: Option<GemKey>,
    /// `Some(dir)` right after a teleport, cleared by any interaction that
    /// doesn't itself teleport.
    last_move: Option<MoveDirection>,
    /// Last rendered terminal-coordinate rect of the gem, for mouse hit tests.
    /// `None` until the Special tab has been drawn at least once.
    pub hit_area: Cell<Option<Rect>>,
}

impl GemState {
    pub fn new() -> Self {
        Self {
            color: PALETTE[0],
            position: GemPosition::Left,
            brand: 0,
            evolved: false,
            shining: false,
            shine_color: PALETTE[0],
            last_key: None,
            last_move: None,
            hit_area: Cell::new(None),
        }
    }

    pub fn color(&self) -> Color {
        self.color
    }

    pub fn position(&self) -> GemPosition {
        self.position
    }

    pub fn brand(&self) -> u8 {
        self.brand
    }

    pub fn evolved(&self) -> bool {
        self.evolved
    }

    pub fn shining(&self) -> bool {
        self.shining
    }

    pub fn shine_color(&self) -> Color {
        self.shine_color
    }

    pub fn last_move(&self) -> Option<MoveDirection> {
        self.last_move
    }

    /// Process a key press. Consecutive presses of the same key are ignored.
    pub fn handle_key(&mut self, key: GemKey) {
        if self.last_key == Some(key) {
            return;
        }
        self.last_key = Some(key);
        self.interact();
    }

    /// Process a mouse click. Mouse clicks aren't subject to the key dedupe
    /// rule, but they do reset it so a subsequent same-as-before key press
    /// counts again.
    pub fn handle_click(&mut self) {
        self.last_key = None;
        self.interact();
    }

    fn interact(&mut self) {
        // Color always changes.
        self.color = random_color_excluding(self.color);

        if self.evolved {
            // Roll a fresh shine on every click.
            self.shining = roll_percent(10);
            if self.shining {
                self.shine_color = random_color_excluding(self.color);
            }
            self.last_move = None;
        } else if roll_percent(33) {
            // Teleport.
            if self.brand >= 9 {
                self.evolved = true;
                self.shining = false;
                self.last_move = None;
            } else {
                self.brand += 1;
                self.position = match self.position {
                    GemPosition::Left => GemPosition::Right,
                    GemPosition::Right => GemPosition::Left,
                };
                self.last_move = Some(match self.position {
                    GemPosition::Left => MoveDirection::Leftward,
                    GemPosition::Right => MoveDirection::Rightward,
                });
            }
        } else {
            // Interaction without a teleport — the gem is sitting still, so
            // the previous trail blows away.
            self.last_move = None;
        }
    }
}

impl Default for GemState {
    fn default() -> Self {
        Self::new()
    }
}

fn roll_percent(threshold: u64) -> bool {
    OsRng.next_u64() % 100 < threshold
}

fn random_color_excluding(current: Color) -> Color {
    loop {
        let idx = (OsRng.next_u64() as usize) % PALETTE.len();
        let candidate = PALETTE[idx];
        if candidate != current {
            return candidate;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_consecutive_keys_are_deduped() {
        let mut gem = GemState::new();
        let initial = gem.color();

        gem.handle_key(GemKey::Space);
        let after_first = gem.color();
        assert_ne!(initial, after_first, "first press must change color");

        // Second identical press is dropped — color stays put.
        gem.handle_key(GemKey::Space);
        assert_eq!(gem.color(), after_first);

        // A different key counts again.
        gem.handle_key(GemKey::J);
        assert_ne!(gem.color(), after_first);
    }

    #[test]
    fn mouse_click_resets_key_dedupe() {
        let mut gem = GemState::new();
        gem.handle_key(GemKey::Space);
        let after_key = gem.color();

        gem.handle_click();
        let after_click = gem.color();
        assert_ne!(after_key, after_click);

        // Same key as before the click now counts because the click reset
        // the dedupe state.
        gem.handle_key(GemKey::Space);
        assert_ne!(gem.color(), after_click);
    }
}
