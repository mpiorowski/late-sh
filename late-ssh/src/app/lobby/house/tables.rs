//! The house-table roster. One enum owns every per-table fact; there is no
//! trait object or registry of managers behind it. Adding a table is: add a
//! variant here, let the compiler walk you through the exhaustive matches
//! (name, tagline, slug, settings, occupancy, client construction in
//! `registry.rs` / `state.rs`), and seed nothing — the chat room and voice
//! channel are ensured idempotently at startup from `ALL`.
//!
//! House tables are fixed by design: one table per variant, no user
//! creation, no settings forms. A second stake tier later is a new enum
//! variant, not config.

use uuid::Uuid;

use crate::app::lobby::house::{
    blackjack::settings::BlackjackTableSettings,
    poker::settings::{PokerPace, PokerTableSettings},
    ssnake::settings::{SsnakeSpeed, SsnakeTableSettings},
    tron::settings::{TronMode, TronSpeed, TronTableSettings},
};
use late_core::models::game_room::GameKind;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HouseTable {
    Poker,
    Blackjack,
    Asterion,
    Tron,
    Ssnake,
}

impl HouseTable {
    /// Roster order: the Lobby modal section and startup seeding follow it.
    pub const ALL: [Self; 5] = [
        Self::Poker,
        Self::Blackjack,
        Self::Asterion,
        Self::Tron,
        Self::Ssnake,
    ];

    /// Stable per-variant runtime id. House tables have no `game_rooms` row;
    /// the singleton services still need a table id for snapshots and client
    /// state, so each variant owns a fixed one.
    pub const fn table_id(self) -> Uuid {
        match self {
            Self::Poker => Uuid::from_u128(0x0000_1a7e_5000_7000_8000_0000_0000_0001),
            Self::Blackjack => Uuid::from_u128(0x0000_1a7e_5000_7000_8000_0000_0000_0002),
            Self::Asterion => Uuid::from_u128(0x0000_1a7e_5000_7000_8000_0000_0000_0003),
            Self::Tron => Uuid::from_u128(0x0000_1a7e_5000_7000_8000_0000_0000_0004),
            Self::Ssnake => Uuid::from_u128(0x0000_1a7e_5000_7000_8000_0000_0000_0005),
        }
    }

    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Poker => "Poker",
            Self::Blackjack => "Blackjack",
            Self::Asterion => "Asterion",
            Self::Tron => "Tron",
            Self::Ssnake => "Super Snake",
        }
    }

    /// One-line pitch for the Lobby modal row.
    pub const fn tagline(self) -> &'static str {
        match self {
            Self::Poker => "hold'em · 1000 stack · 10/20 blinds",
            Self::Blackjack => "house shoe · 10-chip stake",
            Self::Asterion => "escape the maze, dodge the minotaur",
            Self::Tron => "light cycles · quick · glitch",
            Self::Ssnake => "snake arena · warp tunnels · dos classic",
        }
    }

    /// Slug of the table's permanent public `chat_rooms(kind='game')` row.
    pub const fn chat_slug(self) -> &'static str {
        match self {
            Self::Poker => "poker",
            Self::Blackjack => "blackjack",
            Self::Asterion => "maze",
            Self::Tron => "tron",
            Self::Ssnake => "ssnake",
        }
    }

    /// The `chat_rooms.game_kind` value for the seeded chat room. Reuses the
    /// rooms-era kind strings so chat treats house chat exactly like game
    /// chat (hidden from the Home rail, no Mentions, no IRC).
    pub const fn game_kind(self) -> GameKind {
        match self {
            Self::Poker => GameKind::Poker,
            Self::Blackjack => GameKind::Blackjack,
            Self::Asterion => GameKind::Asterion,
            Self::Tron => GameKind::Tron,
            Self::Ssnake => GameKind::Ssnake,
        }
    }

    pub const fn seat_capacity(self) -> usize {
        match self {
            Self::Poker => 4,
            Self::Blackjack => 4,
            Self::Asterion => 12,
            Self::Tron => 4,
            Self::Ssnake => 4,
        }
    }

    /// Fixed house settings. Poker: 1k stack, 10/20 blinds, standard pace.
    /// Blackjack: the 10-chip stake, standard pace. Tron: quick speed,
    /// glitch mode (owner-preserved). Super Snake: classic speed, all four
    /// seats, random arena (seated players cycle it between matches).
    /// Asterion has no settings.
    pub fn poker_settings() -> PokerTableSettings {
        PokerTableSettings {
            pace: PokerPace::Standard,
            small_blind: 10,
            starting_stack: 1_000,
        }
    }

    pub fn blackjack_settings() -> BlackjackTableSettings {
        BlackjackTableSettings::default()
    }

    pub fn tron_settings() -> TronTableSettings {
        TronTableSettings {
            speed: TronSpeed::Quick,
            mode: TronMode::Glitch,
        }
    }

    pub fn ssnake_settings() -> SsnakeTableSettings {
        SsnakeTableSettings {
            speed: SsnakeSpeed::Classic,
            level: None,
            seats: 4,
        }
    }

    pub fn from_chat_slug(slug: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|table| table.chat_slug() == slug)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_ids_are_distinct() {
        for a in HouseTable::ALL {
            for b in HouseTable::ALL {
                if a != b {
                    assert_ne!(a.table_id(), b.table_id());
                }
            }
        }
    }

    #[test]
    fn chat_slugs_round_trip() {
        for table in HouseTable::ALL {
            assert_eq!(HouseTable::from_chat_slug(table.chat_slug()), Some(table));
        }
        assert_eq!(HouseTable::from_chat_slug("chess"), None);
    }

    #[test]
    fn fixed_settings_match_the_locked_decisions() {
        let poker = HouseTable::poker_settings();
        assert_eq!(poker.starting_stack(), 1_000);
        assert_eq!(poker.small_blind(), 10);
        assert_eq!(poker.big_blind(), 20);

        let blackjack = HouseTable::blackjack_settings();
        assert_eq!(blackjack.min_bet(), 10);

        let tron = HouseTable::tron_settings();
        assert_eq!(tron.speed, TronSpeed::Quick);
        assert_eq!(tron.mode, TronMode::Glitch);

        let ssnake = HouseTable::ssnake_settings();
        assert_eq!(ssnake.speed, SsnakeSpeed::Classic);
        assert_eq!(ssnake.level, None);
        assert_eq!(ssnake.seats, 4);
    }
}
