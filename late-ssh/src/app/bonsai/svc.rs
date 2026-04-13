use anyhow::Result;
use chrono::NaiveDate;
use late_core::db::Db;
use late_core::models::bonsai::{Grave, Tree};
use rand_core::{OsRng, RngCore};
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::state::ActivityEvent;

#[derive(Clone)]
pub struct BonsaiService {
    db: Db,
    activity_feed: broadcast::Sender<ActivityEvent>,
}

impl BonsaiService {
    pub fn new(db: Db, activity_feed: broadcast::Sender<ActivityEvent>) -> Self {
        Self { db, activity_feed }
    }

    /// Load or create a bonsai tree for this user. Handles death check on login.
    pub async fn ensure_tree(&self, user_id: Uuid) -> Result<Tree> {
        let client = self.db.get().await?;
        let today = chrono::Utc::now().date_naive();

        if let Some(mut tree) = Tree::find_by_user_id(&client, user_id).await? {
            // Check if tree should die (7+ days without watering)
            // If never watered, use created date as the reference point
            if tree.is_alive {
                let reference_date = tree
                    .last_watered
                    .unwrap_or_else(|| tree.created.date_naive());
                let days_since = (today - reference_date).num_days();
                if days_since >= 7 {
                    let survived = (today - tree.created.date_naive()).num_days().max(0) as i32;
                    Tree::kill(&client, user_id).await?;
                    Grave::record(&client, user_id, survived).await?;
                    tree.is_alive = false;

                    let username =
                        late_core::models::profile::fetch_username(&client, user_id).await;
                    let _ = self.activity_feed.send(ActivityEvent {
                        username,
                        action: format!("lost their bonsai ({survived}d)"),
                        at: std::time::Instant::now(),
                    });
                }
            }
            Ok(tree)
        } else {
            // New user: create tree with user-derived seed
            let seed = user_id.as_u128() as i64;
            let tree = Tree::ensure(&client, user_id, seed).await?;
            Ok(tree)
        }
    }

    /// Water the tree (once per day). Returns true if watering happened.
    pub fn water_task(&self, user_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            if let Err(e) = svc.water(user_id).await {
                tracing::error!(error = ?e, "failed to water bonsai");
            }
        });
    }

    async fn water(&self, user_id: Uuid) -> Result<bool> {
        let client = self.db.get().await?;
        let today = chrono::Utc::now().date_naive();

        let tree = Tree::find_by_user_id(&client, user_id).await?;
        let Some(tree) = tree else {
            return Ok(false);
        };
        if !tree.is_alive {
            return Ok(false);
        }
        if tree.last_watered == Some(today) {
            return Ok(false); // Already watered today
        }

        Tree::water(&client, user_id, today).await?;

        // Grant growth points: base 10, bonus if consecutive day
        let bonus = if let Some(last) = tree.last_watered {
            if (today - last).num_days() == 1 { 5 } else { 0 }
        } else {
            0
        };
        Tree::add_growth(&client, user_id, 10 + bonus).await?;

        // Grant chips for watering
        late_core::models::chips::UserChips::add_bonus(
            &client,
            user_id,
            late_core::models::chips::BONSAI_WATER_BONUS,
        )
        .await?;

        // Broadcast
        let username = late_core::models::profile::fetch_username(&client, user_id).await;
        let _ = self.activity_feed.send(ActivityEvent {
            username,
            action: "watered their bonsai".to_string(),
            at: std::time::Instant::now(),
        });

        Ok(true)
    }

    /// Respawn a dead tree
    pub fn respawn_task(&self, user_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            if let Err(e) = svc.respawn(user_id).await {
                tracing::error!(error = ?e, "failed to respawn bonsai");
            }
        });
    }

    async fn respawn(&self, user_id: Uuid) -> Result<()> {
        let client = self.db.get().await?;
        let new_seed = OsRng.next_u64() as i64;
        Tree::respawn(&client, user_id, new_seed).await?;
        Ok(())
    }

    /// Cut/prune: change seed and subtract growth cost
    pub fn cut_task(&self, user_id: Uuid, new_seed: i64, cost: i32) {
        let svc = self.clone();
        tokio::spawn(async move {
            if let Err(e) = svc.cut(user_id, new_seed, cost).await {
                tracing::error!(error = ?e, "failed to cut bonsai");
            }
        });
    }

    async fn cut(&self, user_id: Uuid, new_seed: i64, cost: i32) -> Result<()> {
        let client = self.db.get().await?;
        Tree::cut(&client, user_id, new_seed, cost).await
    }

    /// Add connection-time growth (called periodically from tick)
    pub fn add_growth_task(&self, user_id: Uuid, points: i32) {
        let svc = self.clone();
        tokio::spawn(async move {
            if let Err(e) = svc.add_growth(user_id, points).await {
                tracing::error!(error = ?e, "failed to add bonsai growth");
            }
        });
    }

    async fn add_growth(&self, user_id: Uuid, points: i32) -> Result<()> {
        let client = self.db.get().await?;
        Tree::add_growth(&client, user_id, points).await
    }

    pub fn today() -> NaiveDate {
        chrono::Utc::now().date_naive()
    }
}
