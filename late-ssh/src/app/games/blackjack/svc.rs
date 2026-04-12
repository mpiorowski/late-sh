use late_core::db::Db;
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::app::games::{blackjack::state::Outcome, chips::svc::ChipService};

#[derive(Clone)]
pub struct BlackjackService {
    chip_svc: ChipService,
    activity_feed: broadcast::Sender<BlackjackEvent>,
    db: Db,
}

pub enum BlackjackEvent {
    BetPlaced {
        room_id: Uuid,
        user_id: Uuid,
        request_id: Uuid,
        result: Result<i64, String>,
    },
    HandSettled {
        room_id: Uuid,
        user_id: Uuid,
        bet: i64,
        outcome: Outcome,
        credit: i64,
        new_balance: i64,
    },
    BetRefunded {
        room_id: Uuid,
        user_id: Uuid,
        amount: i64,
    },
}

impl BlackjackService {
    pub fn new(
        chip_svc: ChipService,
        activity_feed: broadcast::Sender<BlackjackEvent>,
        db: Db,
    ) -> Self {
        Self {
            chip_svc,
            activity_feed,
            db,
        }
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<BlackjackEvent> {
        self.activity_feed.subscribe()
    }

    pub fn place_bet_task(&self, room_id: Uuid, user_id: Uuid, request_id: Uuid, amount: i64) {
        let svc = self.clone();
        tokio::spawn(async move {

                let client = &self.db.get().await?;
                self.chip_svc.subtract_chips(user_id, amount);
        });
    }

    pub fn settle_hand_task(&self, room_id: Uuid, user_id: Uuid, bet: i64, outcome: Outcome) {
        let svc = self.clone();
        tokio::spawn(async move {});
    }

    pub fn refund_bet_task(&self, room_id: Uuid, user_id: Uuid, amount: i64) {
        let svc = self.clone();
        tokio::spawn(async move {});
    }
}
