//! The Chess Puzzle pack: Lichess puzzles (the open database, CC0) cut into
//! three rating tiers by `gen_chess_puzzles`, one JSON file per tier that
//! late-ssh embeds, and the daily pick over a tier.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::models::chips::Difficulty;

/// One puzzle as the pack stores it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChessPuzzle {
    /// The Lichess puzzle id.
    pub id: String,
    /// The position before the opponent's setup move, as Lichess writes it.
    pub fen: String,
    /// The opponent's setup move, then the solution: the player's moves and
    /// the opponent's replies in turn, ending on the player's. Each is a
    /// cozy-chess move string, so a castle is the king taking its own rook
    /// (`e1h1`), the way the board generates it.
    pub moves: Vec<String>,
    pub rating: u16,
    /// The motif that names the puzzle, shown as its hint.
    pub theme: Option<Theme>,
}

/// The Lichess themes a puzzle is named by, in the order the generator
/// prefers them: a mate count first, since it says the most, then the
/// tactic. Lichess tags every puzzle with more (its length, phase, and
/// "crushing"); those name nothing a player can look for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Theme {
    MateIn1,
    MateIn2,
    MateIn3,
    MateIn4,
    Fork,
    Pin,
    Skewer,
    DiscoveredAttack,
    DoubleCheck,
    Deflection,
    Attraction,
    Clearance,
    Interference,
    Intermezzo,
    XRayAttack,
    CapturingDefender,
    TrappedPiece,
    HangingPiece,
    Sacrifice,
    Promotion,
    EnPassant,
    Zugzwang,
    QuietMove,
    DefensiveMove,
}

impl Theme {
    pub const ALL: [Self; 24] = [
        Self::MateIn1,
        Self::MateIn2,
        Self::MateIn3,
        Self::MateIn4,
        Self::Fork,
        Self::Pin,
        Self::Skewer,
        Self::DiscoveredAttack,
        Self::DoubleCheck,
        Self::Deflection,
        Self::Attraction,
        Self::Clearance,
        Self::Interference,
        Self::Intermezzo,
        Self::XRayAttack,
        Self::CapturingDefender,
        Self::TrappedPiece,
        Self::HangingPiece,
        Self::Sacrifice,
        Self::Promotion,
        Self::EnPassant,
        Self::Zugzwang,
        Self::QuietMove,
        Self::DefensiveMove,
    ];

    /// The tag as the Lichess dump spells it.
    pub const fn lichess_tag(self) -> &'static str {
        match self {
            Self::MateIn1 => "mateIn1",
            Self::MateIn2 => "mateIn2",
            Self::MateIn3 => "mateIn3",
            Self::MateIn4 => "mateIn4",
            Self::Fork => "fork",
            Self::Pin => "pin",
            Self::Skewer => "skewer",
            Self::DiscoveredAttack => "discoveredAttack",
            Self::DoubleCheck => "doubleCheck",
            Self::Deflection => "deflection",
            Self::Attraction => "attraction",
            Self::Clearance => "clearance",
            Self::Interference => "interference",
            Self::Intermezzo => "intermezzo",
            Self::XRayAttack => "xRayAttack",
            Self::CapturingDefender => "capturingDefender",
            Self::TrappedPiece => "trappedPiece",
            Self::HangingPiece => "hangingPiece",
            Self::Sacrifice => "sacrifice",
            Self::Promotion => "promotion",
            Self::EnPassant => "enPassant",
            Self::Zugzwang => "zugzwang",
            Self::QuietMove => "quietMove",
            Self::DefensiveMove => "defensiveMove",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::MateIn1 => "Mate in 1",
            Self::MateIn2 => "Mate in 2",
            Self::MateIn3 => "Mate in 3",
            Self::MateIn4 => "Mate in 4",
            Self::Fork => "Fork",
            Self::Pin => "Pin",
            Self::Skewer => "Skewer",
            Self::DiscoveredAttack => "Discovered attack",
            Self::DoubleCheck => "Double check",
            Self::Deflection => "Deflection",
            Self::Attraction => "Attraction",
            Self::Clearance => "Clearance",
            Self::Interference => "Interference",
            Self::Intermezzo => "Intermezzo",
            Self::XRayAttack => "X-ray attack",
            Self::CapturingDefender => "Capture the defender",
            Self::TrappedPiece => "Trapped piece",
            Self::HangingPiece => "Hanging piece",
            Self::Sacrifice => "Sacrifice",
            Self::Promotion => "Promotion",
            Self::EnPassant => "En passant",
            Self::Zugzwang => "Zugzwang",
            Self::QuietMove => "Quiet move",
            Self::DefensiveMove => "Defensive move",
        }
    }

    /// The naming theme among a row's space-separated Lichess tags.
    pub fn from_lichess_tags(tags: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|theme| tags.split(' ').any(|tag| tag == theme.lichess_tag()))
    }
}

/// The Lichess rating band each tier draws from.
pub const fn rating_range(difficulty: Difficulty) -> (u16, u16) {
    match difficulty {
        Difficulty::Easy => (600, 1100),
        Difficulty::Medium => (1300, 1700),
        Difficulty::Hard => (1900, 2300),
    }
}

/// The day's puzzle in a tier: a stable FNV-1a hash of the date and tier,
/// so every session picks the same one without asking the database.
pub fn select_for_date(
    puzzles: &[ChessPuzzle],
    date: NaiveDate,
    difficulty: Difficulty,
) -> &ChessPuzzle {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let key = format!("late-sh-chess-puzzle-daily-v1:{date}:{}", difficulty.key());
    for byte in key.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    &puzzles[(hash % puzzles.len() as u64) as usize]
}
