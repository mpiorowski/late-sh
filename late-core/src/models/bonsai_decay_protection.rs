use anyhow::Result;
use chrono::{DateTime, NaiveDate, Utc};
use tokio_postgres::Client;
use uuid::Uuid;

use super::{
    marketplace::BONSAI_DECAY_PROTECTION_KIND, shop_consumable_effect::ShopConsumableEffect,
};

/// The user's live Bonsai Decay Shield window, if any. Read-side only: the
/// row itself is written by `marketplace::activate_bonsai_decay_protection_in_tx`
/// via `ShopConsumableEffect::extend_user_effect_in_tx`. While the window is
/// live, every calendar day it touches counts as cared-for against both
/// bonsai decay clocks (classic dry-day death, Dynamic vigor/water-stress
/// decay), regardless of watering.
#[derive(Debug, Clone, Copy)]
pub struct BonsaiDecayProtection {
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
}

impl BonsaiDecayProtection {
    pub async fn for_user(client: &Client, user_id: Uuid) -> Result<Option<Self>> {
        let effect = ShopConsumableEffect::active_user_effect_for_user(
            client,
            user_id,
            BONSAI_DECAY_PROTECTION_KIND,
        )
        .await?;
        Ok(effect.map(|effect| Self {
            starts_at: effect.starts_at,
            ends_at: effect.ends_at,
        }))
    }

    /// Whether `day` (a UTC calendar date) falls inside the protection
    /// window's start and end dates.
    pub fn covers_day(&self, day: NaiveDate) -> bool {
        day >= self.starts_at.date_naive() && day <= self.ends_at.date_naive()
    }

    /// How many of the days in `(from, to]` this window covers. Used to
    /// discount a decay clock that counts elapsed calendar days (classic
    /// Bonsai's dry-day death) rather than walking each day individually.
    pub fn protected_days_between(&self, from: NaiveDate, to: NaiveDate) -> i64 {
        if to <= from {
            return 0;
        }
        let range_start = from.succ_opt().unwrap_or(from);
        let window_start = self.starts_at.date_naive().max(range_start);
        let window_end = self.ends_at.date_naive().min(to);
        if window_end < window_start {
            return 0;
        }
        (window_end - window_start).num_days() + 1
    }
}

#[cfg(test)]
#[path = "bonsai_decay_protection_test.rs"]
mod bonsai_decay_protection_test;
