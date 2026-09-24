//! The tailor's mirror: an editor over one `Look`, pick not draw
//! (GAME.md, "The look"). Four rows, hood, eyes, coat, mark; a cursor on
//! one of them; left and right walk the row's rack, `tint` walks the
//! palette, `shuffle` throws the dice the join threw. Pure: the draft is
//! a value, and nothing here knows whether it was worn yet.
//!
//! The rack is the starter set, free, re-picked forever. Bought and
//! earned pieces join the table with their own kinds later and will need
//! an ownership check here; today every piece in `PIECES` is on the rack.

use rand::Rng;

use crate::app::deadchannel::glyphs::GLYPH_ALPHABET;
use crate::app::deadchannel::runner::state::{Look, Slot, TINTS, Worn, pieces_for};

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

/// The look being tried on, and the row the cursor is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Draft {
    pub look: Look,
    pub row: Row,
}

impl Draft {
    pub fn new(look: Look) -> Self {
        Self {
            look,
            row: Row::Hood,
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
                let rack: Vec<&'static _> = pieces_for(slot).collect();
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

    /// The next tint for the row's piece, wrapping. The mark has no tint
    /// (GAME.md: a colored mark is earned, never picked), so on that row
    /// nothing moves.
    pub fn tint(&mut self) {
        let Some(slot) = self.row.slot() else {
            return;
        };
        let worn = self.worn_mut(slot);
        let at = TINTS
            .iter()
            .position(|tint| *tint == worn.tint)
            .expect("the worn tint is in the palette");
        worn.tint = TINTS[wrap(at, 1, TINTS.len())];
    }

    /// A whole new look off the same dice as the join. The cursor stays.
    pub fn shuffle<R: Rng>(&mut self, rng: &mut R) {
        self.look = Look::random(rng);
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
