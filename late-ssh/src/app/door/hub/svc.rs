//! The hub's I/O. One write: a door opened. Every Enter on a card that
//! launches is written to `door_openings` (`late_core::models::door_opening`),
//! after the card's gate and before the connect, so the paper's YESTERDAY
//! column can say how many times the doors were opened. One read: the
//! account's payout timers, when the hub opens, so the landings can say when
//! each monthly milestone pays again. Both run off the input thread; their
//! failures are logged and counted here, since nobody upstream waits for
//! them.

use std::sync::Arc;

use anyhow::Result;
use chrono::Utc;
use late_core::db::Db;
use late_core::models::door_opening::{DoorCard, DoorOpening};
use tokio::sync::oneshot;
use uuid::Uuid;

use crate::app::games::chips::cooldown::CooldownTimers;
use crate::app::games::chips::svc::ChipService;
use crate::metrics::{self, DoorOpeningResult, PayoutTimersRead};
use crate::render_signal::RenderSignal;

#[derive(Clone)]
pub struct GamesHubService {
    db: Db,
    chips: ChipService,
}

impl GamesHubService {
    pub fn new(db: Db, chips: ChipService) -> Self {
        Self { db, chips }
    }

    /// Read `user_id`'s payout timers into `reply` (`hub::state::State`
    /// holds the other end and drains it on tick), and wake the render loop
    /// so the landing shows them on the next frame. A failed read sends
    /// nothing: the state drops the timers it held and the landings print
    /// only the fixed window line.
    pub fn load_timers_task(
        &self,
        user_id: Uuid,
        reply: oneshot::Sender<CooldownTimers>,
        repaint: Option<Arc<RenderSignal>>,
    ) {
        let chips = self.chips.clone();
        tokio::spawn(async move {
            match chips.cooldown_timers(user_id).await {
                Ok(timers) => {
                    metrics::record_payout_timers_read(PayoutTimersRead::Loaded);
                    // A closed receiver is a session that already left the
                    // hub again: nothing to show the timers to.
                    let _ = reply.send(timers);
                    if let Some(signal) = &repaint {
                        signal.wake();
                    }
                }
                Err(err) => {
                    metrics::record_payout_timers_read(PayoutTimersRead::Failed);
                    late_core::error_span!(
                        "payout_timers_read_failed",
                        error = ?err,
                        %user_id,
                        "failed to read the payout timers"
                    );
                }
            }
        });
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
