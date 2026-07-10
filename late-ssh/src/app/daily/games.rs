//! The daily-games roster. One enum owns every per-game fact; there is no
//! trait object or registry behind it. Adding a game is: add a variant here,
//! let the compiler walk you through the exhaustive matches (name, prize,
//! reward key, initial state, move handling, board surface), and seed its
//! win-payout reward template in a migration.

use late_core::models::{
    daily_match::DailyMatch,
    reward::{DAILY_BATTLESHIP_WIN_REWARD_KEY, DAILY_CHESS_WIN_REWARD_KEY},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DailyGame {
    Chess,
    Battleship,
}

impl DailyGame {
    /// Roster order: pickers, help copy, and usage strings follow it.
    pub const ALL: [Self; 2] = [Self::Chess, Self::Battleship];

    /// The persisted `daily_matches.game_kind` value.
    pub const fn kind(self) -> &'static str {
        match self {
            Self::Chess => DailyMatch::GAME_KIND_CHESS,
            Self::Battleship => DailyMatch::GAME_KIND_BATTLESHIP,
        }
    }

    /// Lowercase display name; also what `/challenge` accepts.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Chess => "chess",
            Self::Battleship => "battleship",
        }
    }

    /// Chips the winner takes. This is the displayed number; the credited
    /// amount comes from the game's seeded reward template — keep in sync.
    pub const fn win_payout(self) -> i64 {
        match self {
            Self::Chess => 500,
            Self::Battleship => 300,
        }
    }

    pub const fn reward_key(self) -> &'static str {
        match self {
            Self::Chess => DAILY_CHESS_WIN_REWARD_KEY,
            Self::Battleship => DAILY_BATTLESHIP_WIN_REWARD_KEY,
        }
    }

    pub const fn ledger_reason(self) -> &'static str {
        match self {
            Self::Chess => "daily_chess_win",
            Self::Battleship => "daily_battleship_win",
        }
    }

    /// One-line rules blurb for the board screen's info rail.
    pub const fn tagline(self) -> &'static str {
        match self {
            Self::Chess => "one move per day",
            Self::Battleship => "one salvo per day · a hit fires again",
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

    /// Next game in roster order, wrapping — drives the challenge picker.
    pub fn cycled(self, forward: bool) -> Self {
        let len = Self::ALL.len() as isize;
        let at = Self::ALL.iter().position(|game| *game == self).unwrap_or(0) as isize;
        let step = if forward { 1 } else { -1 };
        Self::ALL[((at + step).rem_euclid(len)) as usize]
    }

    /// `chess|battleship` — for usage banners and help copy.
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
    fn cycled_walks_the_roster_both_ways() {
        assert_eq!(DailyGame::Chess.cycled(true), DailyGame::Battleship);
        assert_eq!(DailyGame::Battleship.cycled(true), DailyGame::Chess);
        assert_eq!(DailyGame::Chess.cycled(false), DailyGame::Battleship);
    }

    #[test]
    fn usage_lists_every_game() {
        assert_eq!(DailyGame::usage_labels(), "chess|battleship");
    }
}
