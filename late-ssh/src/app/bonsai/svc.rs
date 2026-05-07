use anyhow::Result;
use chrono::NaiveDate;
use late_core::db::Db;
use late_core::models::bonsai::{DailyCare, Grave, Tree};
use rand_core::{OsRng, RngCore};
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::state::ActivityEvent;

const MISSED_PRUNE_GROWTH_LOSS: i32 = 10;

#[derive(Clone)]
pub struct BonsaiService {
    db: Db,
    activity_feed: broadcast::Sender<ActivityEvent>,
}

impl BonsaiService {
    pub fn new(db: Db, activity_feed: broadcast::Sender<ActivityEvent>) -> Self {
        Self { db, activity_feed }
    }

    pub async fn ensure_tree(&self, user_id: Uuid) -> Result<Tree> {
        self.ensure_tree_with_care(user_id)
            .await
            .map(|(tree, _care)| tree)
    }

    /// Load or create a bonsai tree and today's UTC care row. Handles death
    /// checks and one-shot missed-care penalties for previous care rows.
    pub async fn ensure_tree_with_care(&self, user_id: Uuid) -> Result<(Tree, DailyCare)> {
        let client = self.db.get().await?;
        let today = chrono::Utc::now().date_naive();

        let mut tree = if let Some(mut tree) = Tree::find_by_user_id(&client, user_id).await? {
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
            tree
        } else {
            // New user: create tree with user-derived seed
            let seed = user_id.as_u128() as i64;
            Tree::ensure(&client, user_id, seed).await?
        };

        if tree.is_alive {
            self.apply_care_penalties(&client, user_id, today, &mut tree)
                .await?;
        }

        let care = DailyCare::ensure(
            &client,
            user_id,
            today,
            crate::app::bonsai::care::branch_goal_for(
                crate::app::bonsai::state::stage_for(tree.is_alive, tree.growth_points),
                tree.seed,
                today,
            ) as i32,
        )
        .await?;
        Ok((tree, care))
    }

    /// Water the tree. Non-admin users are limited to once per day.
    pub fn water_task(&self, user_id: Uuid, unlimited: bool) {
        let svc = self.clone();
        tokio::spawn(async move {
            if let Err(e) = svc.water(user_id, unlimited).await {
                tracing::error!(error = ?e, "failed to water bonsai");
            }
        });
    }

    async fn water(&self, user_id: Uuid, unlimited: bool) -> Result<bool> {
        let client = self.db.get().await?;
        let today = chrono::Utc::now().date_naive();

        if !Tree::water_and_add_growth_if_available(&client, user_id, today, unlimited).await? {
            return Ok(false);
        }
        DailyCare::mark_watered(&client, user_id, today).await?;

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

    pub fn cut_daily_branch_task(&self, user_id: Uuid, care_date: NaiveDate, branch_id: i32) {
        let svc = self.clone();
        tokio::spawn(async move {
            if let Err(e) = svc.cut_daily_branch(user_id, care_date, branch_id).await {
                tracing::error!(error = ?e, "failed to cut daily bonsai branch");
            }
        });
    }

    async fn cut_daily_branch(
        &self,
        user_id: Uuid,
        care_date: NaiveDate,
        branch_id: i32,
    ) -> Result<()> {
        let client = self.db.get().await?;
        DailyCare::add_cut_branch(&client, user_id, care_date, branch_id).await
    }

    pub fn clear_daily_branches_task(&self, user_id: Uuid, care_date: NaiveDate) {
        let svc = self.clone();
        tokio::spawn(async move {
            if let Err(e) = svc.clear_daily_branches(user_id, care_date).await {
                tracing::error!(error = ?e, "failed to reset daily bonsai branches");
            }
        });
    }

    async fn clear_daily_branches(&self, user_id: Uuid, care_date: NaiveDate) -> Result<()> {
        let client = self.db.get().await?;
        DailyCare::clear_cut_branches(&client, user_id, care_date).await
    }

    pub fn reset_daily_care_task(&self, user_id: Uuid, care_date: NaiveDate, branch_goal: i32) {
        let svc = self.clone();
        tokio::spawn(async move {
            if let Err(e) = svc.reset_daily_care(user_id, care_date, branch_goal).await {
                tracing::error!(error = ?e, "failed to reset daily bonsai care");
            }
        });
    }

    async fn reset_daily_care(
        &self,
        user_id: Uuid,
        care_date: NaiveDate,
        branch_goal: i32,
    ) -> Result<()> {
        let client = self.db.get().await?;
        DailyCare::reset_for_respawn(&client, user_id, care_date, branch_goal).await
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

    pub fn lose_growth_task(&self, user_id: Uuid, points: i32) {
        let svc = self.clone();
        tokio::spawn(async move {
            if let Err(e) = svc.lose_growth(user_id, points).await {
                tracing::error!(error = ?e, "failed to subtract bonsai growth");
            }
        });
    }

    async fn lose_growth(&self, user_id: Uuid, points: i32) -> Result<()> {
        let client = self.db.get().await?;
        Tree::lose_growth(&client, user_id, points).await
    }

    pub fn today() -> NaiveDate {
        chrono::Utc::now().date_naive()
    }

    async fn apply_care_penalties(
        &self,
        client: &tokio_postgres::Client,
        user_id: Uuid,
        today: NaiveDate,
        tree: &mut Tree,
    ) -> Result<()> {
        for care in DailyCare::unapplied_before(client, user_id, today).await? {
            let missed_water = !care.watered && !care.water_penalty_applied;
            let missed_prune = (care.cut_branch_ids.len() as i32) < care.branch_goal
                && !care.prune_penalty_applied;

            if missed_prune {
                tree.growth_points = tree.growth_points.saturating_sub(MISSED_PRUNE_GROWTH_LOSS);
                Tree::lose_growth(client, user_id, MISSED_PRUNE_GROWTH_LOSS).await?;
            }

            DailyCare::mark_penalties_applied(
                client,
                user_id,
                care.care_date,
                missed_water,
                missed_prune,
            )
            .await?;
        }
        Ok(())
    }
}
