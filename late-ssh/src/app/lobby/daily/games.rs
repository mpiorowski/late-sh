//! The daily-games roster. One enum owns every per-game fact; there is no
//! trait object or registry behind it. Adding a game is: add a variant here,
//! let the compiler walk you through the exhaustive matches (name, prize,
//! reward key, initial state, move handling, board surface), and seed its
//! win-payout reward template in a migration.

use late_core::models::{
    daily_match::DailyMatch,
    reward::{
        DAILY_BATTLESHIP_WIN_REWARD_KEY, DAILY_CHESS_WIN_REWARD_KEY, DAILY_CONNECT4_WIN_REWARD_KEY,
    },
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DailyGame {
    Chess,
    Battleship,
    ConnectFour,
}

impl DailyGame {
    /// Roster order: pickers, help copy, and usage strings follow it.
    pub const ALL: [Self; 3] = [Self::Chess, Self::Battleship, Self::ConnectFour];

    /// The persisted `daily_matches.game_kind` value.
    pub const fn kind(self) -> &'static str {
        match self {
            Self::Chess => DailyMatch::GAME_KIND_CHESS,
            Self::Battleship => DailyMatch::GAME_KIND_BATTLESHIP,
            Self::ConnectFour => DailyMatch::GAME_KIND_CONNECTFOUR,
        }
    }

    /// Lowercase display name; also what `/challenge` accepts.
    /// The lowercase token used in `/challenge <game>` and usage banners.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Chess => "chess",
            Self::Battleship => "battleship",
            Self::ConnectFour => "connect4",
        }
    }

    /// The human-readable game name for prose surfaces (e.g. the #lounge result
    /// line "beat bob at Connect Four"). Distinct from `label`, which is the
    /// lowercase command token.
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Chess => "Chess",
            Self::Battleship => "Battleship",
            Self::ConnectFour => "Connect Four",
        }
    }

    /// Chips the winner takes. This is the displayed number; the credited
    /// amount comes from the game's seeded reward template — keep in sync.
    pub const fn win_payout(self) -> i64 {
        match self {
            Self::Chess => 500,
            Self::Battleship => 300,
            Self::ConnectFour => 400,
        }
    }

    pub const fn reward_key(self) -> &'static str {
        match self {
            Self::Chess => DAILY_CHESS_WIN_REWARD_KEY,
            Self::Battleship => DAILY_BATTLESHIP_WIN_REWARD_KEY,
            Self::ConnectFour => DAILY_CONNECT4_WIN_REWARD_KEY,
        }
    }

    pub const fn ledger_reason(self) -> &'static str {
        match self {
            Self::Chess => "daily_chess_win",
            Self::Battleship => "daily_battleship_win",
            Self::ConnectFour => "daily_connect4_win",
        }
    }

    /// One-line rules blurb for the board screen's info rail.
    pub const fn tagline(self) -> &'static str {
        match self {
            Self::Chess => "one move per day",
            Self::Battleship => "one salvo per day · a hit fires again",
            Self::ConnectFour => "one drop per day · four in a row wins",
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_round_trip() {
        for game in DailyGame::ALL {
            assert_eq!(DailyGame::from_kind(game.kind()), Some(game));
            assert_eq!(DailyGame::from_label(game.label()), Some(game));
        }
        assert_eq!(DailyGame::from_kind("duel_snake"), None);
        assert_eq!(
            DailyGame::from_label("BATTLESHIP"),
            Some(DailyGame::Battleship)
        );
    }

    #[test]
    fn usage_lists_every_game() {
        assert_eq!(DailyGame::usage_labels(), "chess|battleship|connect4");
    }
}
