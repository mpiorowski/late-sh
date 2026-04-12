use late_core::db::Db;
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::app::games::{
    blackjack::state::{Bet, BetError, MAX_BET, MIN_BET, Outcome},
    chips::svc::ChipService,
};

#[derive(Clone)]
pub struct BlackjackService {
    chip_svc: ChipService,
    event_tx: broadcast::Sender<BlackjackEvent>,
    db: Db,
}

#[derive(Debug, Clone)]
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

#[derive(Debug)]
enum BetFailure {
    BelowMin,
    AboveMax,
    InsufficientChips,
    Internal(anyhow::Error),
}

impl BetFailure {
    fn user_message(&self) -> String {
        match self {
            BetFailure::BelowMin => format!("bet below minimum ({MIN_BET})"),
            BetFailure::AboveMax => format!("bet above maximum ({MAX_BET})"),
            BetFailure::InsufficientChips => "insufficient chips".to_string(),
            BetFailure::Internal(_) => "internal error".to_string(),
        }
    }
}

impl BlackjackService {
    pub fn new(chip_svc: ChipService, event_tx: broadcast::Sender<BlackjackEvent>, db: Db) -> Self {
        Self {
            chip_svc,
            event_tx,
            db,
        }
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<BlackjackEvent> {
        self.event_tx.subscribe()
    }

    pub fn place_bet_task(&self, room_id: Uuid, user_id: Uuid, request_id: Uuid, amount: i64) {
        let svc = self.clone();
        tokio::spawn(async move {
            let result = match svc.place_bet(user_id, amount).await {
                Ok(new_balance) => Ok(new_balance),
                Err(failure) => {
                    if let BetFailure::Internal(ref e) = failure {
                        tracing::error!(
                            error = ?e,
                            %room_id,
                            %user_id,
                            amount,
                            "blackjack place_bet: internal failure"
                        );
                    }
                    Err(failure.user_message())
                }
            };

            if let Err(e) = svc.event_tx.send(BlackjackEvent::BetPlaced {
                room_id,
                user_id,
                request_id,
                result,
            }) {
                tracing::debug!(
                    error = ?e,
                    %room_id,
                    %user_id,
                    "blackjack bet event dropped (no subscribers)"
                );
            }
        });
    }

    async fn place_bet(&self, user_id: Uuid, amount: i64) -> Result<i64, BetFailure> {
        Bet::new(amount).map_err(|e| match e {
            BetError::BelowMin => BetFailure::BelowMin,
            BetError::AboveMax => BetFailure::AboveMax,
        })?;

        match self.chip_svc.debit_bet(user_id, amount).await {
            Ok(Some(new_balance)) => Ok(new_balance),
            Ok(None) => Err(BetFailure::InsufficientChips),
            Err(e) => Err(BetFailure::Internal(e)),
        }
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
