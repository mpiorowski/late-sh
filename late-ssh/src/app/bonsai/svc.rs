use anyhow::Result;
use chrono::NaiveDate;
use late_core::db::Db;
use late_core::models::{
    bonsai::{Tree, TreeParams},
    bonsai_decay_protection::BonsaiDecayProtection,
    chips::{ChipMove, UserChips},
};
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::app::activity::event::ActivityEvent;

pub(crate) const WATER_CHIP_BONUS: i64 = 200;

/// Persistence and side effects for the bonsai. The in-memory
/// `BonsaiState` owns every rule; this only writes what it is handed and
/// pays the daily chips exactly once.
#[derive(Clone)]
pub struct BonsaiService {
    db: Db,
    activity_feed: broadcast::Sender<ActivityEvent>,
}

impl BonsaiService {
    pub fn new(db: Db, activity_feed: broadcast::Sender<ActivityEvent>) -> Self {
        Self { db, activity_feed }
    }

    /// Load the user's tree, planting a fresh seed on the first login, plus
    /// the live Bonsai Decay Shield window so the elapsed-day catch-up can
    /// honour it.
    pub async fn ensure_tree(
        &self,
        user_id: Uuid,
    ) -> Result<(Tree, Option<BonsaiDecayProtection>)> {
        let client = self.db.get().await?;
        let today = Self::today();
        let seed = user_id.as_u128() as i64;
        let graph = crate::app::bonsai::state::seeded_graph_value(seed);
        let badge = crate::app::bonsai::state::seeded_badge_glyph(seed);
        let tree = Tree::ensure(&client, user_id, seed, today, graph, &badge).await?;
        let protection = BonsaiDecayProtection::for_user(&client, user_id).await?;
        Ok((tree, protection))
    }

    /// Persist a watering: the daily gate first, then the full state, then
    /// the chips if this was the first watering of the UTC day. One task,
    /// in that order, so the gate can never lose a race against the save
    /// that stamps the same date.
    pub fn water_task(&self, params: TreeParams) {
        let svc = self.clone();
        tokio::spawn(async move {
            if let Err(e) = svc.water(params).await {
                tracing::error!(error = ?e, "failed to water bonsai");
            }
        });
    }

    async fn water(&self, params: TreeParams) -> Result<()> {
        let client = self.db.get().await?;
        let user_id = params.user_id;
        let today = Self::today();
        let first_today = Tree::water_day(&client, user_id, today).await?;
        Tree::save(&client, params).await?;
        if !first_today {
            return Ok(());
        }
        UserChips::apply(
            &**client,
            user_id,
            ChipMove::BonsaiWatered,
            WATER_CHIP_BONUS,
            &today.to_string(),
        )
        .await?;
        let username = late_core::models::profile::fetch_username(&client, user_id).await;
        let _ = self
            .activity_feed
            .send(ActivityEvent::bonsai_watered(user_id, username));
        Ok(())
    }

    pub fn save_task(&self, params: TreeParams) {
        let svc = self.clone();
        tokio::spawn(async move {
            if let Err(e) = svc.save(params).await {
                tracing::error!(error = ?e, "failed to save bonsai");
            }
        });
    }

    async fn save(&self, params: TreeParams) -> Result<()> {
        let client = self.db.get().await?;
        Tree::save(&client, params).await
    }

    /// The tree died during the elapsed-day catch-up at login. Fire and
    /// forget: the event is private (never shipped to #lounge), it only
    /// feeds the activity stream.
    pub fn lost_task(&self, user_id: Uuid, survived_days: i32) {
        let svc = self.clone();
        tokio::spawn(async move {
            if let Err(e) = svc.lost(user_id, survived_days).await {
                tracing::error!(error = ?e, "failed to announce a lost bonsai");
            }
        });
    }

    async fn lost(&self, user_id: Uuid, survived_days: i32) -> Result<()> {
        let client = self.db.get().await?;
        let username = late_core::models::profile::fetch_username(&client, user_id).await;
        let _ =
            self.activity_feed
                .send(ActivityEvent::bonsai_lost(user_id, username, survived_days));
        Ok(())
    }

    pub fn today() -> NaiveDate {
        chrono::Utc::now().date_naive()
    }
}

#[cfg(test)]
#[path = "svc_test.rs"]
mod svc_test;
