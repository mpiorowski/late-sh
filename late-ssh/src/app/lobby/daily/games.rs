//! The daily-games roster. One enum owns every per-game fact; there is no
//! trait object or registry behind it. Adding a game is: add a variant here,
//! let the compiler walk you through the exhaustive matches (name, prize,
//! reward key, initial state, move handling, board surface), and seed its
//! win-payout reward template in a migration.

use late_core::models::{
    daily_match::DailyMatch,
    reward::{
        DAILY_BACKGAMMON_WIN_REWARD_KEY, DAILY_BATTLESHIP_WIN_REWARD_KEY,
        DAILY_CHECKERS_WIN_REWARD_KEY, DAILY_CHESS_WIN_REWARD_KEY, DAILY_CONNECT4_WIN_REWARD_KEY,
        DAILY_REVERSI_WIN_REWARD_KEY,
    },
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DailyGame {
    Chess,
    Battleship,
    ConnectFour,
    Reversi,
    Checkers,
    Backgammon,
}

impl DailyGame {
    /// Roster order: pickers, help copy, and usage strings follow it.
    pub const ALL: [Self; 6] = [
        Self::Chess,
        Self::Battleship,
        Self::ConnectFour,
        Self::Reversi,
        Self::Checkers,
        Self::Backgammon,
    ];

    /// The persisted `daily_matches.game_kind` value.
    pub const fn kind(self) -> &'static str {
        match self {
            Self::Chess => DailyMatch::GAME_KIND_CHESS,
            Self::Battleship => DailyMatch::GAME_KIND_BATTLESHIP,
            Self::ConnectFour => DailyMatch::GAME_KIND_CONNECTFOUR,
            Self::Reversi => DailyMatch::GAME_KIND_REVERSI,
            Self::Checkers => DailyMatch::GAME_KIND_CHECKERS,
            Self::Backgammon => DailyMatch::GAME_KIND_BACKGAMMON,
        }
    }

    /// Lowercase display name; also what `/challenge` accepts.
    /// The lowercase token used in `/challenge <game>` and usage banners.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Chess => "chess",
            Self::Battleship => "battleship",
            Self::ConnectFour => "connect4",
            Self::Reversi => "reversi",
            Self::Checkers => "checkers",
            Self::Backgammon => "backgammon",
        }
    }

    /// The human-readable game name for prose surfaces (e.g. the #lounge result
    /// line "won a game of Connect Four"). Distinct from `label`, which is the
    /// lowercase command token.
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Chess => "Chess",
            Self::Battleship => "Battleship",
            Self::ConnectFour => "Connect Four",
            Self::Reversi => "Reversi",
            Self::Checkers => "Checkers",
            Self::Backgammon => "Backgammon",
        }
    }

    /// Chips the winner takes. This is the displayed number; the credited
    /// amount comes from the game's seeded reward template — keep in sync.
    pub const fn win_payout(self) -> i64 {
        match self {
            Self::Chess => 500,
            Self::Battleship => 300,
            Self::ConnectFour => 400,
            Self::Reversi => 400,
            Self::Checkers => 400,
            Self::Backgammon => 400,
        }
    }

    pub const fn reward_key(self) -> &'static str {
        match self {
            Self::Chess => DAILY_CHESS_WIN_REWARD_KEY,
            Self::Battleship => DAILY_BATTLESHIP_WIN_REWARD_KEY,
            Self::ConnectFour => DAILY_CONNECT4_WIN_REWARD_KEY,
            Self::Reversi => DAILY_REVERSI_WIN_REWARD_KEY,
            Self::Checkers => DAILY_CHECKERS_WIN_REWARD_KEY,
            Self::Backgammon => DAILY_BACKGAMMON_WIN_REWARD_KEY,
        }
    }

    pub const fn ledger_reason(self) -> &'static str {
        match self {
            Self::Chess => "daily_chess_win",
            Self::Battleship => "daily_battleship_win",
            Self::ConnectFour => "daily_connect4_win",
            Self::Reversi => "daily_reversi_win",
            Self::Checkers => "daily_checkers_win",
            Self::Backgammon => "daily_backgammon_win",
        }
    }

    /// One-line rules blurb for the board screen's info rail.
    pub const fn tagline(self) -> &'static str {
        match self {
            Self::Chess => "one move per day",
            Self::Battleship => "one salvo per day · a hit fires again",
            Self::ConnectFour => "one drop per day · four in a row wins",
            Self::Reversi => "one move per day · most discs wins",
            Self::Checkers => "one move per day · capture or block to win",
            Self::Backgammon => "one roll per day · bear off all fifteen",
        }
    }

    pub fn from_kind(kind: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|game| game.kind() == kind)
    }

    /// Parse a user-typed game name (`/challenge battleship`).
    pub fn from_label(label: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|game| game.label().eq_ignore_ascii_case(label))
    }

    /// Every label joined with `|` — for usage banners and help copy.
    pub fn usage_labels() -> String {
        Self::ALL
            .into_iter()
            .map(Self::label)
            .collect::<Vec<_>>()
            .join("|")
    }
}


