//! The hub's one write: a door opened. Every Enter on a card that launches
//! is written to `door_openings` (`late_core::models::door_opening`), after
//! the card's gate and before the connect, so the paper's YESTERDAY column
//! can say how many times the doors were opened. Fire-and-forget from the
//! input path; the failure is logged and counted here, since nobody
//! upstream waits for it.

use anyhow::Result;
use chrono::Utc;
use late_core::db::Db;
use late_core::models::door_opening::{DoorCard, DoorOpening};
use uuid::Uuid;

use crate::metrics::{self, DoorOpeningResult};

#[derive(Clone)]
pub struct GamesHubService {
    db: Db,
}

impl GamesHubService {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    /// `user_id` opened `card` now.
    pub async fn record(&self, user_id: Uuid, card: DoorCard) -> Result<()> {
        let client = self.db.get().await?;
        DoorOpening::record(&client, user_id, card, Utc::now()).await
    }

    /// The input path's call: the write happens off the input thread, and
    /// a lost row is one opening short in tomorrow's paper, nothing the
    /// session needs to hear about.
    pub fn record_task(&self, user_id: Uuid, card: DoorCard) {
        let service = self.clone();
        tokio::spawn(async move {
            match service.record(user_id, card).await {
                Ok(()) => metrics::record_door_opening(card, DoorOpeningResult::Recorded),
                Err(err) => {
                    metrics::record_door_opening(card, DoorOpeningResult::Failed);
                    late_core::error_span!(
                        "door_opening_failed",
                        error = ?err,
                        %user_id,
                        card = card.key(),
                        "failed to record a door opening"
                    );
                }
            }
        });
    }
}
