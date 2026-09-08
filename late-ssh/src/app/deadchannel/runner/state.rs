//! The runner's look: pieces, tints, and the typed `Look` a runner row
//! carries (GAME.md, "The look" and "The data model"). Art lives here, in
//! code, as a closed table; the database only says which pieces are worn.
//! No I/O, no clock reads.
//!
//! A portrait is three rows of five cells, one slot per row: hood on top,
//! eyes in the middle, coat at the bottom. Rows stack, so any hood composes
//! with any coat and the set needs no compatibility rules. Every row is
//! exactly [`PORTRAIT_WIDTH`] cells with no wide (CJK, emoji) glyph
//! (`state_test` asserts it over the whole table). The rows are block and
//! box-drawing glyphs, East Asian ambiguous width like the rest of the
//! TUI's frames, so a portrait assumes the ambiguous-narrow terminal the
//! whole app already assumes.

use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};

use crate::app::deadchannel::glyphs::GLYPH_ALPHABET;

/// Cells per portrait row.
pub const PORTRAIT_WIDTH: usize = 5;
/// Rows per portrait: one per slot.
pub const PORTRAIT_HEIGHT: usize = 3;

/// The three slots, top to bottom.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    Hood,
    Eyes,
    Coat,
}

/// One piece of a look: its code (what the row stores), the slot it fills,
/// and its five-cell row.
#[derive(Debug, PartialEq, Eq)]
pub struct Piece {
    pub code: &'static str,
    pub slot: Slot,
    pub row: &'static str,
}

/// The starter set: free, assigned at random on the invited join, and
/// re-pickable at the tailor for nothing once the city exists. Bought and
/// earned pieces join this table later with their own kinds.
pub const PIECES: &[Piece] = &[
    // hoods
    Piece {
        code: "hood.plain",
        slot: Slot::Hood,
        row: " ▄▄▄ ",
    },
    Piece {
        code: "hood.heavy",
        slot: Slot::Hood,
        row: " ▟█▙ ",
    },
    Piece {
        code: "hood.cross",
        slot: Slot::Hood,
        row: " ╬═╬ ",
    },
    Piece {
        code: "hood.wire",
        slot: Slot::Hood,
        row: " ┼─┼ ",
    },
    Piece {
        code: "hood.antenna",
        slot: Slot::Hood,
        row: " ╫╫╫ ",
    },
    Piece {
        code: "hood.static",
        slot: Slot::Hood,
        row: " ▚▞▚ ",
    },
    Piece {
        code: "hood.cap",
        slot: Slot::Hood,
        row: " ┌─┐ ",
    },
    Piece {
        code: "hood.flat",
        slot: Slot::Hood,
        row: " ▀▀▀ ",
    },
    Piece {
        code: "hood.crown",
        slot: Slot::Hood,
        row: "▚▞▚▞▚",
    },
    Piece {
        code: "hood.ghost",
        slot: Slot::Hood,
        row: " ░▒░ ",
    },
    // eyes
    Piece {
        code: "eyes.round",
        slot: Slot::Eyes,
        row: "▐● ●▌",
    },
    Piece {
        code: "eyes.dot",
        slot: Slot::Eyes,
        row: "▐▪ ▪▌",
    },
    Piece {
        code: "eyes.gem",
        slot: Slot::Eyes,
        row: "▐◈ ◈▌",
    },
    Piece {
        code: "eyes.square",
        slot: Slot::Eyes,
        row: "▐■ ■▌",
    },
    Piece {
        code: "eyes.visor",
        slot: Slot::Eyes,
        row: "▐═══▌",
    },
    Piece {
        code: "eyes.band",
        slot: Slot::Eyes,
        row: "▐▬▬▬▌",
    },
    Piece {
        code: "eyes.glyph",
        slot: Slot::Eyes,
        row: "▐▚ ▞▌",
    },
    Piece {
        code: "eyes.cross",
        slot: Slot::Eyes,
        row: "▐╳ ╳▌",
    },
    Piece {
        code: "eyes.ghost",
        slot: Slot::Eyes,
        row: " ◌ ◌ ",
    },
    Piece {
        code: "eyes.one",
        slot: Slot::Eyes,
        row: "▐◈ ▪▌",
    },
    // coats
    Piece {
        code: "coat.heavy",
        slot: Slot::Coat,
        row: " ▟▓▙ ",
    },
    Piece {
        code: "coat.solid",
        slot: Slot::Coat,
        row: " ▟█▙ ",
    },
    Piece {
        code: "coat.narrow",
        slot: Slot::Coat,
        row: " ▐▓▌ ",
    },
    Piece {
        code: "coat.thin",
        slot: Slot::Coat,
        row: " ▐█▌ ",
    },
    Piece {
        code: "coat.emblem",
        slot: Slot::Coat,
        row: " ▟╬▙ ",
    },
    Piece {
        code: "coat.bar",
        slot: Slot::Coat,
        row: " ▟═▙ ",
    },
    Piece {
        code: "coat.worn",
        slot: Slot::Coat,
        row: " ▟▒▙ ",
    },
    Piece {
        code: "coat.faded",
        slot: Slot::Coat,
        row: " ▟░▙ ",
    },
    Piece {
        code: "coat.ghost",
        slot: Slot::Coat,
        row: " ▒░▒ ",
    },
    Piece {
        code: "coat.plain",
        slot: Slot::Coat,
        row: " ▟▀▙ ",
    },
];

/// The pieces that fill `slot`, in table order.
pub fn pieces_for(slot: Slot) -> impl Iterator<Item = &'static Piece> {
    PIECES.iter().filter(move |piece| piece.slot == slot)
}

fn piece_by_code(slot: Slot, code: &str) -> Option<&'static Piece> {
    pieces_for(slot).find(|piece| piece.code == code)
}

/// One tint per piece, from a closed palette. Gold is not here on purpose:
/// GAME.md reserves it for earned tint, and a random or bought look must
/// never wear it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tint {
    Static,
    Amber,
    Phosphor,
    White,
    Red,
}

const TINTS: [Tint; 5] = [
    Tint::Static,
    Tint::Amber,
    Tint::Phosphor,
    Tint::White,
    Tint::Red,
];

/// A worn piece: which one, and what color.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Worn {
    pub piece: &'static Piece,
    pub tint: Tint,
}

/// A runner's whole look. Lives on the runner row as JSON (see
/// [`Look::to_json`]) and is parsed back through [`Look::parse`], which
/// rejects unknown codes loudly: a row that names a piece the table does
/// not know is a bug, never a blank.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Look {
    pub hood: Worn,
    pub eyes: Worn,
    pub coat: Worn,
    /// The one-cell mark, drawn from the glyph alphabet. Stored from the
    /// first second so it is fixed at birth; painted once the chat badge
    /// stack learns it (phase 2, build order step 1).
    pub mark: char,
}

/// The stored shape. A separate struct so the JSON stays a plain contract
/// (`{"hood": {"piece": ..., "tint": ...}, ...}`) and the typed `Look` can
/// hold table references.
#[derive(Serialize, Deserialize)]
struct StoredLook {
    hood: StoredWorn,
    eyes: StoredWorn,
    coat: StoredWorn,
    mark: StoredMark,
}

#[derive(Serialize, Deserialize)]
struct StoredWorn {
    piece: String,
    tint: Tint,
}

#[derive(Serialize, Deserialize)]
struct StoredMark {
    glyph: char,
}

#[derive(Debug, PartialEq, Eq)]
pub enum LookError {
    /// The JSON does not have the stored shape at all.
    Shape(String),
    /// A slot names a piece code the table does not know.
    UnknownPiece { slot: Slot, code: String },
    /// The mark is not a character of the glyph alphabet.
    UnknownMark(char),
}

impl std::fmt::Display for LookError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Shape(detail) => write!(f, "look json has the wrong shape: {detail}"),
            Self::UnknownPiece { slot, code } => {
                write!(f, "look names unknown {slot:?} piece {code:?}")
            }
            Self::UnknownMark(glyph) => {
                write!(f, "look mark {glyph:?} is not in the glyph alphabet")
            }
        }
    }
}

impl std::error::Error for LookError {}

impl Look {
    /// A random starter look: one piece per slot, one tint per piece, one
    /// mark from the alphabet. What the invited join assigns.
    pub fn random<R: rand::Rng>(rng: &mut R) -> Self {
        Self {
            hood: random_worn(Slot::Hood, rng),
            eyes: random_worn(Slot::Eyes, rng),
            coat: random_worn(Slot::Coat, rng),
            mark: *GLYPH_ALPHABET
                .choose(rng)
                .expect("glyph alphabet is not empty"),
        }
    }

    pub fn to_json(self) -> serde_json::Value {
        let stored = |worn: Worn| StoredWorn {
            piece: worn.piece.code.to_string(),
            tint: worn.tint,
        };
        serde_json::to_value(StoredLook {
            hood: stored(self.hood),
            eyes: stored(self.eyes),
            coat: stored(self.coat),
            mark: StoredMark { glyph: self.mark },
        })
        .expect("a stored look serializes")
    }

    pub fn parse(value: &serde_json::Value) -> Result<Self, LookError> {
        let stored: StoredLook = match serde_json::from_value(value.clone()) {
            Ok(stored) => stored,
            Err(error) => return Err(LookError::Shape(error.to_string())),
        };
        let worn = |slot: Slot, stored: StoredWorn| match piece_by_code(slot, &stored.piece) {
            Some(piece) => Ok(Worn {
                piece,
                tint: stored.tint,
            }),
            None => Err(LookError::UnknownPiece {
                slot,
                code: stored.piece,
            }),
        };
        let mark = stored.mark.glyph;
        if !GLYPH_ALPHABET.contains(&mark) {
            return Err(LookError::UnknownMark(mark));
        }
        Ok(Self {
            hood: worn(Slot::Hood, stored.hood)?,
            eyes: worn(Slot::Eyes, stored.eyes)?,
            coat: worn(Slot::Coat, stored.coat)?,
            mark,
        })
    }

    /// The worn pieces top to bottom, one per portrait row.
    pub fn rows(&self) -> [Worn; PORTRAIT_HEIGHT] {
        [self.hood, self.eyes, self.coat]
    }
}

fn random_worn<R: rand::Rng>(slot: Slot, rng: &mut R) -> Worn {
    let pieces = pieces_for(slot).collect::<Vec<_>>();
    Worn {
        piece: pieces
            .choose(rng)
            .expect("every slot has at least one starter piece"),
        tint: *TINTS.choose(rng).expect("tint palette is not empty"),
    }
}

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;
