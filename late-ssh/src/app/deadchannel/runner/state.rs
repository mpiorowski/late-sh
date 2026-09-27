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
/// the level that puts it on the tailor's rack, and its five-cell row.
/// Every piece is reachable by level; there is no piece only a few can
/// wear, so no look becomes the one everyone copies.
#[derive(Debug, PartialEq, Eq)]
pub struct Piece {
    pub code: &'static str,
    pub slot: Slot,
    pub level: i32,
    pub row: &'static str,
}

/// Levels that open something at the tailor: three more pieces per slot
/// and a new tint every three levels, white alone at the top of the
/// ladder. `state_test` holds the table and the palette to this list.
pub const UNLOCK_LEVELS: [i32; 6] = [1, 4, 7, 10, 13, 15];

/// The whole rack, ordered by unlock level so the tailor's rack walks from
/// street to legend. Free once unlocked, re-picked forever; the join and
/// the tailor's shuffle draw only from what the level has opened.
pub const PIECES: &[Piece] = &[
    // hoods
    Piece {
        code: "hood.plain",
        slot: Slot::Hood,
        level: 1,
        row: " ▄▄▄ ",
    },
    Piece {
        code: "hood.flat",
        slot: Slot::Hood,
        level: 1,
        row: " ▀▀▀ ",
    },
    Piece {
        code: "hood.cap",
        slot: Slot::Hood,
        level: 1,
        row: " ┌─┐ ",
    },
    Piece {
        code: "hood.heavy",
        slot: Slot::Hood,
        level: 4,
        row: " ▟█▙ ",
    },
    Piece {
        code: "hood.wire",
        slot: Slot::Hood,
        level: 4,
        row: " ┼─┼ ",
    },
    Piece {
        code: "hood.hat",
        slot: Slot::Hood,
        level: 4,
        row: " ▛▀▜ ",
    },
    Piece {
        code: "hood.antenna",
        slot: Slot::Hood,
        level: 7,
        row: " ╫╫╫ ",
    },
    Piece {
        code: "hood.static",
        slot: Slot::Hood,
        level: 7,
        row: " ▚▞▚ ",
    },
    Piece {
        code: "hood.frame",
        slot: Slot::Hood,
        level: 7,
        row: " ╔═╗ ",
    },
    Piece {
        code: "hood.ghost",
        slot: Slot::Hood,
        level: 10,
        row: " ░▒░ ",
    },
    Piece {
        code: "hood.tuner",
        slot: Slot::Hood,
        level: 10,
        row: " ╪═╪ ",
    },
    Piece {
        code: "hood.spikes",
        slot: Slot::Hood,
        level: 10,
        row: " ┳┳┳ ",
    },
    Piece {
        code: "hood.cross",
        slot: Slot::Hood,
        level: 13,
        row: " ╬═╬ ",
    },
    Piece {
        code: "hood.crown",
        slot: Slot::Hood,
        level: 13,
        row: "▚▞▚▞▚",
    },
    Piece {
        code: "hood.halo",
        slot: Slot::Hood,
        level: 13,
        row: "▗▄▄▄▖",
    },
    // eyes
    Piece {
        code: "eyes.dot",
        slot: Slot::Eyes,
        level: 1,
        row: "▐▪ ▪▌",
    },
    Piece {
        code: "eyes.round",
        slot: Slot::Eyes,
        level: 1,
        row: "▐● ●▌",
    },
    Piece {
        code: "eyes.square",
        slot: Slot::Eyes,
        level: 1,
        row: "▐■ ■▌",
    },
    Piece {
        code: "eyes.band",
        slot: Slot::Eyes,
        level: 4,
        row: "▐▬▬▬▌",
    },
    Piece {
        code: "eyes.cross",
        slot: Slot::Eyes,
        level: 4,
        row: "▐╳ ╳▌",
    },
    Piece {
        code: "eyes.slit",
        slot: Slot::Eyes,
        level: 4,
        row: "▐─ ─▌",
    },
    Piece {
        code: "eyes.visor",
        slot: Slot::Eyes,
        level: 7,
        row: "▐═══▌",
    },
    Piece {
        code: "eyes.glyph",
        slot: Slot::Eyes,
        level: 7,
        row: "▐▚ ▞▌",
    },
    Piece {
        code: "eyes.one",
        slot: Slot::Eyes,
        level: 7,
        row: "▐◈ ▪▌",
    },
    Piece {
        code: "eyes.ghost",
        slot: Slot::Eyes,
        level: 10,
        row: " ◌ ◌ ",
    },
    Piece {
        code: "eyes.noise",
        slot: Slot::Eyes,
        level: 10,
        row: "▐░▒░▌",
    },
    Piece {
        code: "eyes.test",
        slot: Slot::Eyes,
        level: 10,
        row: "▐▓░▓▌",
    },
    Piece {
        code: "eyes.gem",
        slot: Slot::Eyes,
        level: 13,
        row: "▐◈ ◈▌",
    },
    Piece {
        code: "eyes.black",
        slot: Slot::Eyes,
        level: 13,
        row: "▐█ █▌",
    },
    Piece {
        code: "eyes.signal",
        slot: Slot::Eyes,
        level: 13,
        row: "▐◈▬◈▌",
    },
    // coats
    Piece {
        code: "coat.plain",
        slot: Slot::Coat,
        level: 1,
        row: " ▟▀▙ ",
    },
    Piece {
        code: "coat.thin",
        slot: Slot::Coat,
        level: 1,
        row: " ▐█▌ ",
    },
    Piece {
        code: "coat.narrow",
        slot: Slot::Coat,
        level: 1,
        row: " ▐▓▌ ",
    },
    Piece {
        code: "coat.solid",
        slot: Slot::Coat,
        level: 4,
        row: " ▟█▙ ",
    },
    Piece {
        code: "coat.heavy",
        slot: Slot::Coat,
        level: 4,
        row: " ▟▓▙ ",
    },
    Piece {
        code: "coat.faded",
        slot: Slot::Coat,
        level: 4,
        row: " ▟░▙ ",
    },
    Piece {
        code: "coat.worn",
        slot: Slot::Coat,
        level: 7,
        row: " ▟▒▙ ",
    },
    Piece {
        code: "coat.bar",
        slot: Slot::Coat,
        level: 7,
        row: " ▟═▙ ",
    },
    Piece {
        code: "coat.wire",
        slot: Slot::Coat,
        level: 7,
        row: " ▟┼▙ ",
    },
    Piece {
        code: "coat.ghost",
        slot: Slot::Coat,
        level: 10,
        row: " ▒░▒ ",
    },
    Piece {
        code: "coat.cross",
        slot: Slot::Coat,
        level: 10,
        row: " ▟╳▙ ",
    },
    Piece {
        code: "coat.long",
        slot: Slot::Coat,
        level: 10,
        row: "▟▓▓▓▙",
    },
    Piece {
        code: "coat.mantle",
        slot: Slot::Coat,
        level: 13,
        row: "▟███▙",
    },
    Piece {
        code: "coat.black",
        slot: Slot::Coat,
        level: 13,
        row: " ███ ",
    },
    Piece {
        code: "coat.drift",
        slot: Slot::Coat,
        level: 13,
        row: "▟▓▒▓▙",
    },
];

/// The pieces that fill `slot`, in table order.
pub fn pieces_for(slot: Slot) -> impl Iterator<Item = &'static Piece> {
    PIECES.iter().filter(move |piece| piece.slot == slot)
}

/// The pieces that fill `slot` a runner at `level` may wear.
pub fn unlocked_pieces(slot: Slot, level: i32) -> impl Iterator<Item = &'static Piece> {
    pieces_for(slot).filter(move |piece| piece.level <= level)
}

/// The tints a runner at `level` may wear, in rack order.
pub fn unlocked_tints(level: i32) -> impl Iterator<Item = Tint> {
    TINTS.into_iter().filter(move |tint| tint.level() <= level)
}

/// The next level above `level` that opens something; `None` at the top.
pub fn next_unlock(level: i32) -> Option<i32> {
    UNLOCK_LEVELS.into_iter().find(|unlock| *unlock > level)
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
    Cyan,
    Magenta,
    Red,
    White,
}

impl Tint {
    /// The level that puts this tint on the tailor's rack.
    pub fn level(self) -> i32 {
        match self {
            Tint::Static => 1,
            Tint::Amber => 1,
            Tint::Phosphor => 4,
            Tint::Cyan => 7,
            Tint::Magenta => 10,
            Tint::Red => 13,
            Tint::White => 15,
        }
    }

    /// The tint's name as the tailor prints it: the stored name
    /// (`state_test` pins the two together).
    pub fn name(self) -> &'static str {
        match self {
            Tint::Static => "static",
            Tint::Amber => "amber",
            Tint::Phosphor => "phosphor",
            Tint::Cyan => "cyan",
            Tint::Magenta => "magenta",
            Tint::Red => "red",
            Tint::White => "white",
        }
    }
}

/// The palette in rack order, which is unlock order: what the tailor
/// cycles through.
pub const TINTS: [Tint; 7] = [
    Tint::Static,
    Tint::Amber,
    Tint::Phosphor,
    Tint::Cyan,
    Tint::Magenta,
    Tint::Red,
    Tint::White,
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
    /// A random look from what `level` has unlocked: one piece per slot,
    /// one tint per piece, one mark from the alphabet. The invited join
    /// throws it at level 1; the tailor's shuffle at the runner's level.
    pub fn random<R: rand::Rng>(level: i32, rng: &mut R) -> Self {
        Self {
            hood: random_worn(Slot::Hood, level, rng),
            eyes: random_worn(Slot::Eyes, level, rng),
            coat: random_worn(Slot::Coat, level, rng),
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

fn random_worn<R: rand::Rng>(slot: Slot, level: i32, rng: &mut R) -> Worn {
    let pieces = unlocked_pieces(slot, level).collect::<Vec<_>>();
    let tints = unlocked_tints(level).collect::<Vec<_>>();
    Worn {
        piece: pieces
            .choose(rng)
            .expect("level 1 unlocks pieces in every slot"),
        tint: *tints.choose(rng).expect("level 1 unlocks tints"),
    }
}

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;
