use std::sync::Arc;

use tokio::sync::{Mutex, broadcast, watch};
use uuid::Uuid;

use crate::app::games::{
    blackjack::state::{
        Bet, BetError, BlackjackSnapshot, MAX_BET, MIN_BET, Outcome, Phase, Shoe, dealer_must_hit,
        is_bust, is_natural_blackjack, payout_credit, score, settle,
    },
    cards::PlayingCard,
    chips::svc::ChipService,
};

#[derive(Clone)]
pub struct BlackjackService {
    chip_svc: ChipService,
    snapshot_tx: watch::Sender<BlackjackSnapshot>,
    snapshot_rx: watch::Receiver<BlackjackSnapshot>,
    event_tx: broadcast::Sender<BlackjackEvent>,
    table: Arc<Mutex<SharedTableState>>,
}

#[derive(Debug, Clone)]
pub enum BlackjackEvent {
    BetPlaced {
        user_id: Uuid,
        request_id: Uuid,
        result: Result<i64, String>,
    },
    HandSettled {
        user_id: Uuid,
        bet: i64,
        outcome: Outcome,
        credit: i64,
        new_balance: i64,
    },
    ActionError {
        user_id: Uuid,
        message: String,
    },
}

#[derive(Debug)]
enum BetFailure {
    BelowMin,
    AboveMax,
    TableBusy,
    InsufficientChips,
    Internal(anyhow::Error),
}

impl BetFailure {
    fn user_message(&self) -> String {
        match self {
            BetFailure::BelowMin => format!("bet below minimum ({MIN_BET})"),
            BetFailure::AboveMax => format!("bet above maximum ({MAX_BET})"),
            BetFailure::TableBusy => "table is busy".to_string(),
            BetFailure::InsufficientChips => "insufficient chips".to_string(),
            BetFailure::Internal(_) => "internal error".to_string(),
        }
    }
}

#[derive(Debug)]
enum ActionFailure {
    NotYourTurn,
    InvalidPhase(&'static str),
    Internal(anyhow::Error),
}

impl ActionFailure {
    fn user_message(&self) -> String {
        match self {
            ActionFailure::NotYourTurn => "it is not your turn".to_string(),
            ActionFailure::InvalidPhase(msg) => (*msg).to_string(),
            ActionFailure::Internal(_) => "internal error".to_string(),
        }
    }
}

impl BlackjackService {
    pub fn new(chip_svc: ChipService, event_tx: broadcast::Sender<BlackjackEvent>) -> Self {
        let initial_snapshot = SharedTableState::new().snapshot();
        let (snapshot_tx, snapshot_rx) = watch::channel(initial_snapshot);
        Self {
            chip_svc,
            snapshot_tx,
            snapshot_rx,
            event_tx,
            table: Arc::new(Mutex::new(SharedTableState::new())),
        }
    }

    pub fn subscribe_state(&self) -> watch::Receiver<BlackjackSnapshot> {
        self.snapshot_rx.clone()
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<BlackjackEvent> {
        self.event_tx.subscribe()
    }

    pub fn place_bet_task(&self, user_id: Uuid, request_id: Uuid, amount: i64) {
        let svc = self.clone();
        tokio::spawn(async move {
            let result = match svc.place_bet(user_id, amount).await {
                Ok(new_balance) => Ok(new_balance),
                Err(failure) => {
                    if let BetFailure::Internal(ref e) = failure {
                        tracing::error!(error = ?e, %user_id, amount, "blackjack place_bet failed");
                    }
                    Err(failure.user_message())
                }
            };
            let _ = svc.event_tx.send(BlackjackEvent::BetPlaced {
                user_id,
                request_id,
                result,
            });
        });
    }

    pub fn hit_task(&self, user_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            if let Err(failure) = svc.hit(user_id).await {
                if let ActionFailure::Internal(ref e) = failure {
                    tracing::error!(error = ?e, %user_id, "blackjack hit failed");
                }
                let _ = svc.event_tx.send(BlackjackEvent::ActionError {
                    user_id,
                    message: failure.user_message(),
                });
            }
        });
    }

    pub fn stand_task(&self, user_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            if let Err(failure) = svc.stand(user_id).await {
                if let ActionFailure::Internal(ref e) = failure {
                    tracing::error!(error = ?e, %user_id, "blackjack stand failed");
                }
                let _ = svc.event_tx.send(BlackjackEvent::ActionError {
                    user_id,
                    message: failure.user_message(),
                });
            }
        });
    }

    pub fn next_hand_task(&self, user_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            if let Err(failure) = svc.next_hand(user_id).await {
                if let ActionFailure::Internal(ref e) = failure {
                    tracing::error!(error = ?e, %user_id, "blackjack next_hand failed");
                }
                let _ = svc.event_tx.send(BlackjackEvent::ActionError {
                    user_id,
                    message: failure.user_message(),
                });
            }
        });
    }

    async fn place_bet(&self, user_id: Uuid, amount: i64) -> Result<i64, BetFailure> {
        Bet::new(amount).map_err(|e| match e {
            BetError::BelowMin => BetFailure::BelowMin,
            BetError::AboveMax => BetFailure::AboveMax,
        })?;

        {
            let mut table = self.table.lock().await;
            if table.phase != Phase::Betting {
                return Err(BetFailure::TableBusy);
            }
            table.active_player_id = Some(user_id);
            table.phase = Phase::BetPending;
            table.status_message = format!("Placing bet: {amount} chips...");
            self.publish_snapshot_locked(&table);
        }

        let new_balance = match self.chip_svc.debit_bet(user_id, amount).await {
            Ok(Some(new_balance)) => new_balance,
            Ok(None) => {
                let mut table = self.table.lock().await;
                table.reset_to_betting("insufficient chips");
                self.publish_snapshot_locked(&table);
                return Err(BetFailure::InsufficientChips);
            }
            Err(e) => {
                let mut table = self.table.lock().await;
                table.reset_to_betting("internal error");
                self.publish_snapshot_locked(&table);
                return Err(BetFailure::Internal(e));
            }
        };

        let settlement = {
            let mut table = self.table.lock().await;
            table.bet = Some(Bet::new(amount).expect("validated bet"));
            table.start_hand();
            let settlement = table.current_settlement();
            self.publish_snapshot_locked(&table);
            settlement
        };

        if let Some((bet, outcome, credit)) = settlement {
            self.persist_settlement(user_id, bet.amount(), outcome, credit)
                .await
                .map_err(BetFailure::Internal)?;
        }

        Ok(new_balance)
    }

    async fn hit(&self, user_id: Uuid) -> Result<(), ActionFailure> {
        let settlement = {
            let mut table = self.table.lock().await;
            table.ensure_active_player(user_id)?;
            if table.phase != Phase::PlayerTurn {
                return Err(ActionFailure::InvalidPhase("you cannot hit right now"));
            }
            let card = table.shoe.draw();
            table.player_hand.push(card);
            if is_bust(&table.player_hand) {
                table.finish_hand(Outcome::DealerWin);
            } else {
                table.status_message =
                    format!("You hit. Total: {}.", score(&table.player_hand).total);
            }
            let settlement = table.current_settlement();
            self.publish_snapshot_locked(&table);
            settlement
        };

        if let Some((bet, outcome, credit)) = settlement {
            self.persist_settlement(user_id, bet.amount(), outcome, credit)
                .await
                .map_err(ActionFailure::Internal)?;
        }

        Ok(())
    }

    async fn stand(&self, user_id: Uuid) -> Result<(), ActionFailure> {
        let settlement = {
            let mut table = self.table.lock().await;
            table.ensure_active_player(user_id)?;
            if table.phase != Phase::PlayerTurn {
                return Err(ActionFailure::InvalidPhase("you cannot stand right now"));
            }
            table.phase = Phase::DealerTurn;
            table.status_message = "Dealer's turn.".to_string();
            while dealer_must_hit(&table.dealer_hand) {
                let card = table.shoe.draw();
                table.dealer_hand.push(card);
            }
            let outcome = settle(&table.player_hand, &table.dealer_hand);
            table.finish_hand(outcome);
            let settlement = table.current_settlement();
            self.publish_snapshot_locked(&table);
            settlement
        };

        if let Some((bet, outcome, credit)) = settlement {
            self.persist_settlement(user_id, bet.amount(), outcome, credit)
                .await
                .map_err(ActionFailure::Internal)?;
        }

        Ok(())
    }

    async fn next_hand(&self, user_id: Uuid) -> Result<(), ActionFailure> {
        let mut table = self.table.lock().await;
        table.ensure_active_player(user_id)?;
        if table.phase != Phase::Settling {
            return Err(ActionFailure::InvalidPhase("hand is still in progress"));
        }
        table.reset_to_betting(&format!("Place a bet ({MIN_BET}-{MAX_BET} chips)."));
        self.publish_snapshot_locked(&table);
        Ok(())
    }

    async fn persist_settlement(
        &self,
        user_id: Uuid,
        bet: i64,
        outcome: Outcome,
        credit: i64,
    ) -> anyhow::Result<()> {
        if credit == 0 {
            return Ok(());
        }
        let new_balance = self.chip_svc.credit_payout(user_id, credit).await?;
        let _ = self.event_tx.send(BlackjackEvent::HandSettled {
            user_id,
            bet,
            outcome,
            credit,
            new_balance,
        });
        Ok(())
    }

    fn publish_snapshot_locked(&self, table: &SharedTableState) {
        let _ = self.snapshot_tx.send(table.snapshot());
    }
}

struct SharedTableState {
    shoe: Shoe,
    dealer_hand: Vec<PlayingCard>,
    player_hand: Vec<PlayingCard>,
    bet: Option<Bet>,
    phase: Phase,
    active_player_id: Option<Uuid>,
    last_outcome: Option<Outcome>,
    last_net_change: i64,
    status_message: String,
}

impl SharedTableState {
    fn new() -> Self {
        Self {
            shoe: Shoe::new(),
            dealer_hand: Vec::new(),
            player_hand: Vec::new(),
            bet: None,
            phase: Phase::Betting,
            active_player_id: None,
            last_outcome: None,
            last_net_change: 0,
            status_message: format!("Place a bet ({MIN_BET}-{MAX_BET} chips)."),
        }
    }

    fn snapshot(&self) -> BlackjackSnapshot {
        BlackjackSnapshot {
            balance: 0,
            dealer_hand: self.dealer_hand.clone(),
            player_hand: self.player_hand.clone(),
            current_bet_amount: self.bet.map(Bet::amount),
            phase: self.phase,
            last_outcome: self.last_outcome,
            last_net_change: self.last_net_change,
            bet_input: String::new(),
            status_message: self.status_message.clone(),
            dealer_revealed: matches!(self.phase, Phase::DealerTurn | Phase::Settling),
            dealer_score: if matches!(self.phase, Phase::DealerTurn | Phase::Settling) {
                Some(score(&self.dealer_hand))
            } else {
                None
            },
            player_score: if self.player_hand.is_empty() {
                None
            } else {
                Some(score(&self.player_hand))
            },
            outcome_banner: self.outcome_banner(),
            active_player_id: self.active_player_id,
        }
    }

    fn start_hand(&mut self) {
        self.dealer_hand.clear();
        self.player_hand.clear();
        self.last_outcome = None;
        self.last_net_change = 0;

        self.player_hand.push(self.shoe.draw());
        self.dealer_hand.push(self.shoe.draw());
        self.player_hand.push(self.shoe.draw());
        self.dealer_hand.push(self.shoe.draw());

        let player_blackjack = is_natural_blackjack(&self.player_hand);
        let dealer_blackjack = is_natural_blackjack(&self.dealer_hand);
        if player_blackjack || dealer_blackjack {
            self.finish_hand(settle(&self.player_hand, &self.dealer_hand));
        } else {
            self.phase = Phase::PlayerTurn;
            self.status_message = "Hit or stand.".to_string();
        }
    }

    fn finish_hand(&mut self, outcome: Outcome) {
        let Some(bet) = self.bet else {
            self.reset_to_betting(&format!("Place a bet ({MIN_BET}-{MAX_BET} chips)."));
            return;
        };
        let credit = payout_credit(bet, outcome);
        self.phase = Phase::Settling;
        self.last_outcome = Some(outcome);
        self.last_net_change = credit - bet.amount();
        self.status_message = match outcome {
            Outcome::PlayerBlackjack => "Blackjack pays 3:2.".to_string(),
            Outcome::PlayerWin => "You beat the dealer.".to_string(),
            Outcome::Push => "Push. Bet returned.".to_string(),
            Outcome::DealerWin if is_bust(&self.player_hand) => "You busted.".to_string(),
            Outcome::DealerWin => "Dealer takes the hand.".to_string(),
        };
    }

    fn current_settlement(&self) -> Option<(Bet, Outcome, i64)> {
        if self.phase != Phase::Settling {
            return None;
        }
        let outcome = self.last_outcome?;
        let bet = self.bet?;
        Some((bet, outcome, payout_credit(bet, outcome)))
    }

    fn reset_to_betting(&mut self, status: &str) {
        self.dealer_hand.clear();
        self.player_hand.clear();
        self.bet = None;
        self.phase = Phase::Betting;
        self.active_player_id = None;
        self.last_outcome = None;
        self.last_net_change = 0;
        self.status_message = status.to_string();
    }

    fn ensure_active_player(&self, user_id: Uuid) -> Result<(), ActionFailure> {
        match self.active_player_id {
            Some(active) if active == user_id => Ok(()),
            _ => Err(ActionFailure::NotYourTurn),
        }
    }

    fn outcome_banner(&self) -> Option<(String, String)> {
        let outcome = self.last_outcome?;
        let subtitle = match outcome {
            Outcome::PlayerBlackjack | Outcome::PlayerWin => format!("+{}", self.last_net_change),
            Outcome::Push => "Bet returned".to_string(),
            Outcome::DealerWin => "No payout".to_string(),
        };
        let title = match outcome {
            Outcome::PlayerBlackjack => "BLACKJACK!",
            Outcome::PlayerWin => "You win!",
            Outcome::Push => "Push",
            Outcome::DealerWin if is_bust(&self.player_hand) => "Bust",
            Outcome::DealerWin => "Dealer wins",
        };
        Some((title.to_string(), subtitle))
    }
}
