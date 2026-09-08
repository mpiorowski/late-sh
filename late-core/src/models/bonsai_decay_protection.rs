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
/// live, every calendar day it touches counts as cared-for by the bonsai's
/// vigor/water-stress simulation, regardless of watering.
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
}

#[cfg(test)]
#[path = "bonsai_decay_protection_test.rs"]
mod bonsai_decay_protection_test;
