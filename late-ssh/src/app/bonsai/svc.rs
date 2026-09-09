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

    /// Load the user's tree, planting a fresh seed on the first login.
    pub async fn ensure_tree(&self, user_id: Uuid) -> Result<Tree> {
        let client = self.db.get().await?;
        let today = Self::today();
        let seed = user_id.as_u128() as i64;
        let graph = crate::app::bonsai::state::seeded_graph_value(seed);
        let badge = crate::app::bonsai::state::seeded_badge_glyph(seed);
        Tree::ensure(&client, user_id, seed, today, graph, &badge).await
    }

    /// The live Bonsai Decay Shield window, so the elapsed-day catch-up can
    /// honour it. Separate from `ensure_tree` so a failure here degrades to
    /// "no shield" at bootstrap instead of discarding a tree that loaded.
    pub async fn decay_protection(&self, user_id: Uuid) -> Result<Option<BonsaiDecayProtection>> {
        let client = self.db.get().await?;
        BonsaiDecayProtection::for_user(&client, user_id).await
    }

    /// Persist a watering. The daily gate and the chip credit commit in one
    /// transaction, so they succeed or fail together; the full state save
    /// follows on its own. `Tree::save` never writes `last_watered`, so a
    /// save from another action landing first cannot pre-empt the gate.
    pub fn water_task(&self, params: TreeParams) {
        let svc = self.clone();
        tokio::spawn(async move {
            if let Err(e) = svc.water(params).await {
                tracing::error!(error = ?e, "failed to water bonsai");
            }
        });
    }

    async fn water(&self, params: TreeParams) -> Result<()> {
        let mut client = self.db.get().await?;
        let user_id = params.user_id;
        let today = Self::today();
        let tx = client.transaction().await?;
        let first_today = Tree::water_day(&*tx, user_id, today).await?;
        if first_today {
            UserChips::apply(
                &*tx,
                user_id,
                ChipMove::BonsaiWatered,
                WATER_CHIP_BONUS,
                &today.to_string(),
            )
            .await?;
        }
        tx.commit().await?;
        Tree::save(&client, params).await?;
        if !first_today {
            return Ok(());
        }
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

    /// The selection cursor moved: a one-column write, not a graph save.
    pub fn select_branch_task(&self, user_id: Uuid, selected_branch_id: Option<i32>) {
        let svc = self.clone();
        tokio::spawn(async move {
            if let Err(e) = svc.select_branch(user_id, selected_branch_id).await {
                tracing::error!(error = ?e, "failed to save bonsai selection");
            }
        });
    }

    async fn select_branch(&self, user_id: Uuid, selected_branch_id: Option<i32>) -> Result<()> {
        let client = self.db.get().await?;
        Tree::select_branch(&client, user_id, selected_branch_id).await
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
