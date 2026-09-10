use anyhow::{Result, bail};
use chrono::{NaiveDate, Utc};
use late_core::db::Db;
use late_core::models::{
    aquarium_care::{self as care_rules, AquariumCare},
    aquarium_shield::AquariumShield,
    chips::{ChipMove, UserChips},
    marketplace,
};
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::app::activity::event::ActivityEvent;

pub(crate) const FEED_CHIP_BONUS: i64 = 100;

/// What a session learns about its tank at connect: the care row (`None`
/// for a user with no tank), the newest shield window, and the fish the
/// starvation settlement took just now.
#[derive(Debug, Clone, Default)]
pub struct CareBootstrap {
    pub care: Option<AquariumCare>,
    pub shield: Option<AquariumShield>,
    pub lost: Vec<String>,
}

/// Persistence and side effects for the tank's care, `BonsaiService::water`
/// twice over. `state::AquariumCare` owns every rule the screen needs; this
/// writes what it is handed, pays the daily chips exactly once, hatches the
/// fry the streak earns, and settles starvation at login.
#[derive(Clone)]
pub struct AquariumService {
    db: Db,
    activity_feed: broadcast::Sender<ActivityEvent>,
}

impl AquariumService {
    pub fn new(db: Db, activity_feed: broadcast::Sender<ActivityEvent>) -> Self {
        Self { db, activity_feed }
    }

    /// The connect-time read and the starvation settlement in one call. A
    /// tank owner gets a care row if they never had one (the clock starts
    /// now), then every `CARE_DAYS` unfed days since the last meal that are
    /// not settled yet cost one swimming fish, picked by weight (cheap fish
    /// first, a Bigbert rarely). The row is locked for the transaction, so
    /// two devices or two replicas connecting at once take turns and the
    /// second finds nothing left to settle. Users without a tank only get
    /// the read.
    pub async fn bootstrap(&self, user_id: Uuid) -> Result<CareBootstrap> {
        let mut client = self.db.get().await?;
        if !marketplace::user_owns_aquarium(&**client, user_id).await? {
            return Ok(CareBootstrap {
                care: AquariumCare::load(&**client, user_id).await?,
                shield: AquariumShield::latest_for_user(&**client, user_id).await?,
                lost: Vec::new(),
            });
        }

        let today = Utc::now().date_naive();
        let tx = client.transaction().await?;
        AquariumCare::ensure(&*tx, user_id).await?;
        let Some(care) = AquariumCare::lock(&*tx, user_id).await? else {
            bail!("aquarium care row missing right after ensure");
        };
        let shield = AquariumShield::latest_for_user(&*tx, user_id).await?;
        let dry = care_rules::dry_days(care.last_fed.date_naive(), today, shield.as_ref());
        let due = care_rules::deaths_due(dry) as i32;
        let mut lost = Vec::new();
        if due > care.deaths_settled {
            let mut stock = marketplace::swimming_fish_in_tx(&tx, user_id).await?;
            for _ in care.deaths_settled..due {
                let Some(victim) = care_rules::pick_by_weight(&stock, rand::random()) else {
                    break;
                };
                let item_id = victim.item_id;
                let creature = victim.creature.clone();
                marketplace::starve_aquarium_fish_in_tx(&tx, user_id, item_id).await?;
                if let Some(fish) = stock.iter_mut().find(|fish| fish.item_id == item_id) {
                    fish.active_quantity -= 1;
                    fish.quantity -= 1;
                }
                lost.push(creature);
            }
            AquariumCare::settle_deaths(&*tx, user_id, due).await?;
            if !lost.is_empty() {
                marketplace::notify_user_shop_changed(&*tx, user_id).await?;
            }
        }
        tx.commit().await?;

        if !lost.is_empty() {
            let username = late_core::models::profile::fetch_username(&client, user_id).await;
            for creature in &lost {
                tracing::info!(
                    user_id = %user_id,
                    username = %username,
                    creature = %creature,
                    dry_days = dry,
                    "aquarium fish starved"
                );
                let _ = self.activity_feed.send(ActivityEvent::aquarium_fish_lost(
                    user_id,
                    username.clone(),
                    creature.clone(),
                ));
            }
        }
        Ok(CareBootstrap {
            care: Some(AquariumCare {
                deaths_settled: due.max(care.deaths_settled),
                ..care
            }),
            shield,
            lost,
        })
    }

    /// Persist a feeding. The daily gate, the chip credit, and any fry the
    /// streak earns commit in one transaction, so they succeed or fail
    /// together; the activity events follow only when the gate said this was
    /// the first feed of the day.
    pub fn feed_task(&self, user_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            if let Err(e) = svc.feed(user_id).await {
                tracing::error!(error = ?e, user_id = %user_id, "failed to feed aquarium");
            }
        });
    }

    async fn feed(&self, user_id: Uuid) -> Result<()> {
        self.feed_on(user_id, Utc::now().date_naive()).await
    }

    async fn feed_on(&self, user_id: Uuid, today: NaiveDate) -> Result<()> {
        let mut client = self.db.get().await?;
        let tx = client.transaction().await?;
        let Some(streak) = AquariumCare::feed_day(&*tx, user_id, today).await? else {
            tx.commit().await?;
            return Ok(());
        };
        UserChips::apply(
            &*tx,
            user_id,
            ChipMove::AquariumFed,
            FEED_CHIP_BONUS,
            &today.to_string(),
        )
        .await?;
        let mut hatched: Option<(String, bool)> = None;
        if care_rules::hatches_fry(streak) {
            let stock = marketplace::swimming_fish_in_tx(&tx, user_id).await?;
            if let Some(parent) = care_rules::pick_by_weight(&stock, rand::random()) {
                let swimming =
                    marketplace::hatch_aquarium_fry_in_tx(&tx, user_id, parent.item_id).await?;
                if swimming {
                    AquariumCare::set_fry(&*tx, user_id, &parent.creature, today).await?;
                }
                marketplace::notify_user_shop_changed(&*tx, user_id).await?;
                hatched = Some((parent.creature.clone(), swimming));
            }
        }
        tx.commit().await?;

        let username = late_core::models::profile::fetch_username(&client, user_id).await;
        let _ = self
            .activity_feed
            .send(ActivityEvent::aquarium_fed(user_id, username.clone()));
        if let Some((creature, swimming)) = hatched {
            tracing::info!(
                user_id = %user_id,
                username = %username,
                creature = %creature,
                streak,
                swimming,
                "aquarium fry hatched"
            );
            let _ = self.activity_feed.send(ActivityEvent::aquarium_fry_hatched(
                user_id, username, creature, swimming,
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "svc_test.rs"]
mod svc_test;
