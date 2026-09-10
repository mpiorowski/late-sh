//! The Aquarium Shield window: an auto feeder that minds the tank. A day the
//! window covers counts as neither fed nor unfed for the care clocks, so no
//! fish starves and the water stays clean while the owner is away.

use anyhow::Result;
use chrono::{DateTime, NaiveDate, Utc};
use tokio_postgres::{Client, GenericClient};
use uuid::Uuid;

use super::{marketplace::AQUARIUM_SHIELD_KIND, shop_consumable_effect::ShopConsumableEffect};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AquariumShield {
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
}

impl AquariumShield {
    /// The live window, if any: what the shop shows and sells against.
    pub async fn for_user(client: &Client, user_id: Uuid) -> Result<Option<Self>> {
        let effect =
            ShopConsumableEffect::active_user_effect_for_user(client, user_id, AQUARIUM_SHIELD_KIND)
                .await?;
        Ok(effect.map(Self::from_effect))
    }

    /// The newest window, live or lapsed. The starvation clock needs this
    /// one: a shield that ran out last week still excuses the days it
    /// covered, and `for_user` would have forgotten it.
    pub async fn latest_for_user(
        client: &impl GenericClient,
        user_id: Uuid,
    ) -> Result<Option<Self>> {
        let effect = ShopConsumableEffect::latest_user_effect_for_user(
            client,
            user_id,
            AQUARIUM_SHIELD_KIND,
        )
        .await?;
        Ok(effect.map(Self::from_effect))
    }

    fn from_effect(effect: ShopConsumableEffect) -> Self {
        Self {
            starts_at: effect.starts_at,
            ends_at: effect.ends_at,
        }
    }

    /// Whether `day` (a UTC calendar date) falls inside the window's start
    /// and end dates.
    pub fn covers_day(&self, day: NaiveDate) -> bool {
        day >= self.starts_at.date_naive() && day <= self.ends_at.date_naive()
    }
}
