//! The tailor's mirror: an editor over one `Look`, pick not draw
//! (GAME.md, "The look"). Four rows, hood, eyes, coat, mark; a cursor on
//! one of them; left and right walk the row's rack, `tint` walks the
//! palette, `shuffle` throws the dice the join threw. Pure: the draft is
//! a value, and nothing here knows whether it was worn yet.
//!
//! The draft knows the runner's peak level, and the peak is the gate: the
//! rack holds only the pieces and tints it has unlocked (`Piece::level`,
//! `Tint::level`), free, re-picked forever. The peak only climbs (an Old
//! Signal reset takes the level, never the peak), so what a runner wears
//! is always on their rack.

use rand::Rng;

use crate::app::deadchannel::glyphs::GLYPH_ALPHABET;
use crate::app::deadchannel::runner::state::{Look, Slot, Worn, unlocked_pieces, unlocked_tints};

/// The rows of the mirror, top to bottom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Row {
    Hood,
    Eyes,
    Coat,
    Mark,
}

const ROWS: [Row; 4] = [Row::Hood, Row::Eyes, Row::Coat, Row::Mark];

impl Row {
    /// The slot a row dresses; the mark has none.
    pub fn slot(self) -> Option<Slot> {
        match self {
            Row::Hood => Some(Slot::Hood),
            Row::Eyes => Some(Slot::Eyes),
            Row::Coat => Some(Slot::Coat),
            Row::Mark => None,
        }
    }
}

/// The look being tried on, the row the cursor is on, and the peak level
/// the racks are cut to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Draft {
    pub look: Look,
    pub row: Row,
    pub level: i32,
}

impl Draft {
    pub fn new(look: Look, level: i32) -> Self {
        Self {
            look,
            row: Row::Hood,
            level,
        }
    }

    /// The cursor one row up; the top holds.
    pub fn up(&mut self) {
        let at = ROWS.iter().position(|row| *row == self.row).expect("a row");
        self.row = ROWS[at.saturating_sub(1)];
    }

    /// The cursor one row down; the bottom holds.
    pub fn down(&mut self) {
        let at = ROWS.iter().position(|row| *row == self.row).expect("a row");
        self.row = ROWS[(at + 1).min(ROWS.len() - 1)];
    }

    /// The next thing on the row's rack, wrapping.
    pub fn next(&mut self) {
        self.step(1);
    }

    /// The previous thing on the row's rack, wrapping.
    pub fn prev(&mut self) {
        self.step(-1);
    }

    fn step(&mut self, by: i32) {
        match self.row.slot() {
            Some(slot) => {
                let rack: Vec<&'static _> = unlocked_pieces(slot, self.level).collect();
                let worn = self.worn_mut(slot);
                let at = rack
                    .iter()
                    .position(|piece| *piece == worn.piece)
                    .expect("the worn piece is on the rack");
                worn.piece = rack[wrap(at, by, rack.len())];
            }
            None => {
                let at = GLYPH_ALPHABET
                    .iter()
                    .position(|glyph| *glyph == self.look.mark)
                    .expect("the mark is in the alphabet");
                self.look.mark = GLYPH_ALPHABET[wrap(at, by, GLYPH_ALPHABET.len())];
            }
        }
    }

    /// The next unlocked tint for the row's piece, wrapping. The mark has no tint
    /// (GAME.md: a colored mark is earned, never picked), so on that row
    /// nothing moves.
    pub fn tint(&mut self) {
        let Some(slot) = self.row.slot() else {
            return;
        };
        let tints: Vec<_> = unlocked_tints(self.level).collect();
        let worn = self.worn_mut(slot);
        let at = tints
            .iter()
            .position(|tint| *tint == worn.tint)
            .expect("the worn tint is unlocked");
        worn.tint = tints[wrap(at, 1, tints.len())];
    }

    /// A whole new look off the join's dice, cut to the level. The cursor
    /// stays.
    pub fn shuffle<R: Rng>(&mut self, rng: &mut R) {
        self.look = Look::random(self.level, rng);
    }

    /// What the row on the cursor wears, for the rack view; `None` on the
    /// mark row.
    pub fn worn(&self, row: Row) -> Option<Worn> {
        match row {
            Row::Hood => Some(self.look.hood),
            Row::Eyes => Some(self.look.eyes),
            Row::Coat => Some(self.look.coat),
            Row::Mark => None,
        }
    }

    fn worn_mut(&mut self, slot: Slot) -> &mut Worn {
        match slot {
            Slot::Hood => &mut self.look.hood,
            Slot::Eyes => &mut self.look.eyes,
            Slot::Coat => &mut self.look.coat,
        }
    }
}

/// `at + by` on a ring of `len`.
fn wrap(at: usize, by: i32, len: usize) -> usize {
    let len = len as i32;
    ((at as i32 + by).rem_euclid(len)) as usize
}

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;
