//! DB-backed persistence for Traffic per-track scores and the aggregate high
//! score (sum of a user's per-track bests).

use anyhow::Result;
use late_core::db::Db;
use late_core::models::traffic::{HighScore, TrackScore};
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::app::activity::event::{ActivityEvent, ActivityGame};
use crate::app::activity::publisher::ActivityPublisher;

#[derive(Clone)]
pub struct TrafficService {
    db: Db,
    activity: Option<ActivityPublisher>,
}

impl TrafficService {
    pub fn new(db: Db) -> Self {
        Self { db, activity: None }
    }

    pub fn with_activity_feed(mut self, activity_feed: broadcast::Sender<ActivityEvent>) -> Self {
        self.activity = Some(ActivityPublisher::new(self.db.clone(), activity_feed));
        self
    }

    pub async fn load_track_scores(&self, user_id: Uuid) -> Result<Vec<TrackScore>> {
        let client = self.db.get().await?;
        TrackScore::list_for_user(&client, user_id).await
    }

    pub async fn load_high_score(&self, user_id: Uuid) -> Result<Option<HighScore>> {
        let client = self.db.get().await?;
        HighScore::find_by_user_id(&client, user_id).await
    }

    /// Persist one finished track's score (kept only if higher), recompute the
    /// aggregate total, record a score event, and publish quest activity.
    pub fn submit_track_score_task(&self, user_id: Uuid, track_key: String, score: i32) {
        let svc = self.clone();
        tokio::spawn(async move {
            if let Err(e) = svc.submit_track_score(user_id, track_key, score).await {
                tracing::error!(error = ?e, "failed to submit traffic track score");
            }
        });
    }

    async fn submit_track_score(&self, user_id: Uuid, track_key: String, score: i32) -> Result<()> {
        let mut client = self.db.get().await?;
        let total =
            HighScore::update_track_score_if_higher(&mut client, user_id, &track_key, score)
                .await?;
        HighScore::record_score_event(&client, user_id, total).await?;
        if let Some(activity) = &self.activity {
            activity.game_scored_task(user_id, ActivityGame::Traffic, total, None);
        }
        Ok(())
    }
}
