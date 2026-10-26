//! Every door opened from the Games hub (`door_openings`, migration 260):
//! the write, and the closed roster of cards the rows are keyed by.
//!
//! A row is one Enter on a hub card that launches, by one account, at one
//! moment: late-ssh's `app/door/hub` writes it after the card's gate and
//! before the connect. A member who quits and comes back opens the door
//! again, so a day's rows count openings, never runs or people. The
//! paper's YESTERDAY column reads them per card through
//! `leaderboard::day_tallies` (`DayGame::Door`).

use anyhow::Result;
use chrono::{DateTime, Utc};
use deadpool_postgres::GenericClient;
use uuid::Uuid;

/// The hub's cards that launch something, as `door_openings.card` stores
/// them. The hub's own roster (`late-ssh/src/app/door/hub/state.rs`,
/// `HubGame`) maps onto this one card by card, the two info-only server
/// cards to none; a card that is not here cannot be written down.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DoorCard {
    NightCity,
    Lateania,
    Realm,
    Rebels,
    Nethack,
    Dcss,
    Brogue,
    Usurper,
    GreenDragon,
    Dopewars,
    Bashquest,
    Codekeep,
    Zork,
    Darkroom,
}

impl DoorCard {
    pub const ALL: [Self; 14] = [
        Self::NightCity,
        Self::Lateania,
        Self::Realm,
        Self::Rebels,
        Self::Nethack,
        Self::Dcss,
        Self::Brogue,
        Self::Usurper,
        Self::GreenDragon,
        Self::Dopewars,
        Self::Bashquest,
        Self::Codekeep,
        Self::Zork,
        Self::Darkroom,
    ];

    /// The stored key. The three roguelikes share theirs with
    /// `leaderboard::DoorGame::key`, which the day's tallies rely on.
    pub const fn key(self) -> &'static str {
        match self {
            Self::NightCity => "night_city",
            Self::Lateania => "lateania",
            Self::Realm => "realm",
            Self::Rebels => "rebels",
            Self::Nethack => "nethack",
            Self::Dcss => "dcss",
            Self::Brogue => "brogue",
            Self::Usurper => "usurper",
            Self::GreenDragon => "greendragon",
            Self::Dopewars => "dopewars",
            Self::Bashquest => "bashquest",
            Self::Codekeep => "codekeep",
            Self::Zork => "zork",
            Self::Darkroom => "darkroom",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|card| card.key() == key)
    }

    /// The card's name as the hub prints it, for the paper's Doors line.
    /// The roguelikes' match `leaderboard::DoorGame::title`.
    pub const fn title(self) -> &'static str {
        match self {
            Self::NightCity => "Night City",
            Self::Lateania => "Lateania",
            Self::Realm => "Realm",
            Self::Rebels => "Rebels",
            Self::Nethack => "NetHack",
            Self::Dcss => "DCSS",
            Self::Brogue => "Brogue",
            Self::Usurper => "Usurper",
            Self::GreenDragon => "Green Dragon",
            Self::Dopewars => "dopewars",
            Self::Bashquest => "BashQuest",
            Self::Codekeep => "CodeKeep",
            Self::Zork => "Zork Trilogy",
            Self::Darkroom => "A Dark Room",
        }
    }
}

pub struct DoorOpening;

impl DoorOpening {
    /// Write `user_id` down as having opened `card` at `opened_at`.
    pub async fn record(
        client: &impl GenericClient,
        user_id: Uuid,
        card: DoorCard,
        opened_at: DateTime<Utc>,
    ) -> Result<()> {
        client
            .execute(
                "INSERT INTO door_openings (user_id, card, opened_at) VALUES ($1, $2, $3)",
                &[&user_id, &card.key(), &opened_at],
            )
            .await?;
        Ok(())
    }
}
