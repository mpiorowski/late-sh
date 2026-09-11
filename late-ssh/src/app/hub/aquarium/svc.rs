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
/// for a user with no tank), every shield window ever bought, the fish the
/// starvation settlement took just now, and what the sprout clock did: a
/// sprout that rooted while the owner was away (`Some(swimming)`), and
/// whether a new one came up.
#[derive(Debug, Clone, Default)]
pub struct CareBootstrap {
    pub care: Option<AquariumCare>,
    pub shields: Vec<AquariumShield>,
    pub lost: Vec<String>,
    pub rooted: Option<bool>,
    pub sprouted: bool,
}

/// What a feed press came to once the DB had its say.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FeedOutcome {
    /// First meal of the UTC day: chips paid, streak advanced.
    Fed,
    /// The tank already ate today; nothing written.
    AlreadyFedToday,
    /// The user owns no tank. The UI never sends this, so it is logged.
    NoTank,
}

/// What a cut press came to once the DB had its say.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CutOutcome {
    /// The sprout is gone.
    Cut,
    /// No sprout young enough to cut: bare floor, or it rooted already.
    NothingToCut,
    /// The user owns no tank. The UI never sends this, so it is logged.
    NoTank,
}

/// Persistence and side effects for the tank's care, `BonsaiService::water`
/// twice over. `state::AquariumCare` owns every rule the screen needs; this
/// writes what it is handed, pays the daily chips exactly once, hatches the
/// fry the streak earns, cuts the sprout, and settles starvation and the
/// sprout clock at login.
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
    /// second finds nothing left to settle. The sprout clock settles in the
    /// same transaction: a sprout past its week roots as a plant, then a
    /// due sprout comes up on the bare floor. Users without a tank only get
    /// the read.
    pub async fn bootstrap(&self, user_id: Uuid) -> Result<CareBootstrap> {
        let mut client = self.db.get().await?;
        if !marketplace::user_owns_aquarium(&**client, user_id).await? {
            return Ok(CareBootstrap {
                care: AquariumCare::load(&**client, user_id).await?,
                shields: AquariumShield::all_for_user(&**client, user_id).await?,
                lost: Vec::new(),
                rooted: None,
                sprouted: false,
            });
        }

        let today = Utc::now().date_naive();
        let tx = client.transaction().await?;
        AquariumCare::ensure(&*tx, user_id).await?;
        let Some(care) = AquariumCare::lock(&*tx, user_id).await? else {
            bail!("aquarium care row missing right after ensure");
        };
        let shields = AquariumShield::all_for_user(&*tx, user_id).await?;
        let dry = care_rules::dry_days(care.last_fed.date_naive(), today, &shields);
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
        let (rooted, sprouted) = settle_sprout_clock_in_tx(&tx, user_id, today).await?;
        tx.commit().await?;

        if !lost.is_empty() || rooted.is_some() || sprouted {
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
            self.announce_sprout_clock(user_id, &username, rooted, sprouted, today);
        }
        Ok(CareBootstrap {
            care: Some(AquariumCare {
                deaths_settled: due.max(care.deaths_settled),
                sprout_born: if sprouted {
                    Some(today)
                } else if rooted.is_some() {
                    None
                } else {
                    care.sprout_born
                },
                ..care
            }),
            shields,
            lost,
            rooted,
            sprouted,
        })
    }

    /// The sprout clock on the UTC day edge, for a session that stays up
    /// across it: a sprout past its week roots, a due sprout comes up, and
    /// the events tell every session of the owner's, this one included. The
    /// connect does the same inside the bootstrap; a session asks once per
    /// day (`state::AquariumCare::take_sprout_settlement_on`).
    pub fn settle_sprout_clock_task(&self, user_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            if let Err(e) = svc.settle_sprout_clock(user_id).await {
                tracing::error!(error = ?e, user_id = %user_id, "failed to settle aquarium sprout clock");
            }
        });
    }

    async fn settle_sprout_clock(&self, user_id: Uuid) -> Result<()> {
        let mut client = self.db.get().await?;
        if !marketplace::user_owns_aquarium(&**client, user_id).await? {
            return Ok(());
        }
        let today = Utc::now().date_naive();
        let tx = client.transaction().await?;
        let (rooted, sprouted) = settle_sprout_clock_in_tx(&tx, user_id, today).await?;
        tx.commit().await?;
        if rooted.is_some() || sprouted {
            let username = late_core::models::profile::fetch_username(&client, user_id).await;
            self.announce_sprout_clock(user_id, &username, rooted, sprouted, today);
        }
        Ok(())
    }

    /// The sprout clock's news on the activity feed: the rooting (with
    /// whether the plant went into the water) and the new sprout.
    fn announce_sprout_clock(
        &self,
        user_id: Uuid,
        username: &str,
        rooted: Option<bool>,
        sprouted: bool,
        today: NaiveDate,
    ) {
        if let Some(swimming) = rooted {
            tracing::info!(
                user_id = %user_id,
                username = %username,
                swimming,
                "aquarium sprout rooted"
            );
            let _ = self
                .activity_feed
                .send(ActivityEvent::aquarium_sprout_rooted(
                    user_id,
                    username.to_string(),
                    swimming,
                ));
        }
        if sprouted {
            let _ = self.activity_feed.send(ActivityEvent::aquarium_sprouted(
                user_id,
                username.to_string(),
                today,
            ));
        }
    }

    /// Persist a cut. The ownership check and the sprout's own gate (still
    /// standing, still young enough) decide in one statement; the event
    /// follows only when this press was the one that cut it, so a second
    /// device clears its floor too.
    pub fn cut_task(&self, user_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            match svc.cut(user_id).await {
                Ok(CutOutcome::Cut) | Ok(CutOutcome::NothingToCut) => {}
                Ok(CutOutcome::NoTank) => {
                    tracing::warn!(user_id = %user_id, "aquarium cut without a tank");
                }
                Err(e) => {
                    tracing::error!(error = ?e, user_id = %user_id, "failed to cut aquarium sprout");
                }
            }
        });
    }

    async fn cut(&self, user_id: Uuid) -> Result<CutOutcome> {
        self.cut_on(user_id, Utc::now().date_naive()).await
    }

    async fn cut_on(&self, user_id: Uuid, today: NaiveDate) -> Result<CutOutcome> {
        let client = self.db.get().await?;
        if !marketplace::user_owns_aquarium(&**client, user_id).await? {
            return Ok(CutOutcome::NoTank);
        }
        if !AquariumCare::cut_sprout(&**client, user_id, today).await? {
            return Ok(CutOutcome::NothingToCut);
        }
        let username = late_core::models::profile::fetch_username(&client, user_id).await;
        let _ = self
            .activity_feed
            .send(ActivityEvent::aquarium_sprout_cut(user_id, username));
        Ok(CutOutcome::Cut)
    }

    /// Persist a feeding. The ownership check, the daily gate, the chip
    /// credit, and any fry the streak earns commit in one transaction, so
    /// they succeed or fail together; the activity events follow only when
    /// the gate said this was the first feed of the day.
    pub fn feed_task(&self, user_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            match svc.feed(user_id).await {
                Ok(FeedOutcome::Fed) | Ok(FeedOutcome::AlreadyFedToday) => {}
                Ok(FeedOutcome::NoTank) => {
                    tracing::warn!(user_id = %user_id, "aquarium feed without a tank");
                }
                Err(e) => {
                    tracing::error!(error = ?e, user_id = %user_id, "failed to feed aquarium");
                }
            }
        });
    }

    async fn feed(&self, user_id: Uuid) -> Result<FeedOutcome> {
        self.feed_on(user_id, Utc::now().date_naive()).await
    }

    async fn feed_on(&self, user_id: Uuid, today: NaiveDate) -> Result<FeedOutcome> {
        let mut client = self.db.get().await?;
        let tx = client.transaction().await?;
        // The chips reward tending a tank; without one there is nothing to
        // tend and no clock to start.
        if !marketplace::user_owns_aquarium(&*tx, user_id).await? {
            tx.commit().await?;
            return Ok(FeedOutcome::NoTank);
        }
        let shields = AquariumShield::all_for_user(&*tx, user_id).await?;
        let continues_from = care_rules::streak_continues_from(today, &shields);
        let Some(streak) = AquariumCare::feed_day(&*tx, user_id, today, continues_from).await?
        else {
            tx.commit().await?;
            return Ok(FeedOutcome::AlreadyFedToday);
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
        Ok(FeedOutcome::Fed)
    }
}

#[cfg(test)]
#[path = "svc_test.rs"]
mod svc_test;

/// The sprout clock inside a transaction: a sprout past its week roots as
/// a wigglewort (`Some(swimming)`), then a due sprout comes up on the bare
/// floor (`true`). The row's own gates make both idempotent.
async fn settle_sprout_clock_in_tx(
    tx: &tokio_postgres::Transaction<'_>,
    user_id: Uuid,
    today: NaiveDate,
) -> Result<(Option<bool>, bool)> {
    let mut rooted = None;
    if AquariumCare::root_sprout(&*tx, user_id, today).await? {
        let swimming = marketplace::root_aquarium_sprout_in_tx(tx, user_id).await?;
        marketplace::notify_user_shop_changed(&*tx, user_id).await?;
        rooted = Some(swimming);
    }
    let sprouted = AquariumCare::sprout_up(&*tx, user_id, today).await?;
    Ok((rooted, sprouted))
}
