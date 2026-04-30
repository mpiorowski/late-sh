use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use tokio::sync::{Mutex, broadcast, watch};
use uuid::Uuid;

use crate::app::{
    games::{cards::PlayingCard, chips::svc::ChipService},
    rooms::blackjack::{
        player::{BlackjackPlayerDirectory, BlackjackPlayerInfo},
        settings::BlackjackTableSettings,
        state::{
            Bet, BlackjackSeat, BlackjackSnapshot, MAX_SEATS, Outcome, Phase, SeatPhase, Shoe,
            dealer_must_hit, is_bust, is_natural_blackjack, payout_credit, score, settle,
        },
    },
};

const BETTING_LOCK_CAP_SECS: u64 = 60;

#[derive(Clone)]
pub struct BlackjackService {
    chip_svc: ChipService,
    player_directory: BlackjackPlayerDirectory,
    snapshot_tx: watch::Sender<BlackjackSnapshot>,
    snapshot_rx: watch::Receiver<BlackjackSnapshot>,
    event_tx: broadcast::Sender<BlackjackEvent>,
    table: Arc<Mutex<SharedTableState>>,
}

#[derive(Debug, Clone)]
pub enum BlackjackEvent {
    SeatJoined {
        user_id: Uuid,
        seat_index: usize,
    },
    SeatLeft {
        user_id: Uuid,
        seat_index: usize,
    },
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
    BelowMin(i64),
    AboveMax(i64),
    NotSeated,
    AlreadyBet,
    TableBusy,
    NoStake,
    InsufficientChips,
    Internal(anyhow::Error),
}

impl BetFailure {
    fn user_message(&self) -> String {
        match self {
            BetFailure::BelowMin(min) => format!("bet below minimum ({min})"),
            BetFailure::AboveMax(max) => format!("bet above maximum ({max})"),
            BetFailure::NotSeated => "sit before betting".to_string(),
            BetFailure::AlreadyBet => "bet already placed".to_string(),
            BetFailure::TableBusy => "table is busy".to_string(),
            BetFailure::NoStake => "throw chips onto the stake first".to_string(),
            BetFailure::InsufficientChips => "insufficient chips".to_string(),
            BetFailure::Internal(_) => "internal error".to_string(),
        }
    }
}

#[derive(Debug)]
enum StakeFailure {
    InvalidChip,
    AboveMax(i64),
    NotSeated,
    AlreadyBet,
    TableBusy,
}

impl StakeFailure {
    fn user_message(&self) -> String {
        match self {
            StakeFailure::InvalidChip => "invalid chip".to_string(),
            StakeFailure::AboveMax(max) => format!("table max is {max} chips"),
            StakeFailure::NotSeated => "sit before betting".to_string(),
            StakeFailure::AlreadyBet => "bet already placed".to_string(),
            StakeFailure::TableBusy => "table is busy".to_string(),
        }
    }
}

#[derive(Debug)]
enum ActionFailure {
    InvalidPhase(&'static str),
    NotSeated,
    Internal(anyhow::Error),
}

impl ActionFailure {
    fn user_message(&self) -> String {
        match self {
            ActionFailure::InvalidPhase(msg) => (*msg).to_string(),
            ActionFailure::NotSeated => "sit before playing".to_string(),
            ActionFailure::Internal(_) => "internal error".to_string(),
        }
    }
}

#[derive(Debug)]
enum SeatFailure {
    AlreadySeated,
    TableFull,
    NotSeated,
    CannotLeaveWithBet,
}

impl SeatFailure {
    fn user_message(&self) -> String {
        match self {
            SeatFailure::AlreadySeated => "you are already seated".to_string(),
            SeatFailure::TableFull => "table is full".to_string(),
            SeatFailure::NotSeated => "you are not seated".to_string(),
            SeatFailure::CannotLeaveWithBet => {
                "finish the round before leaving your seat".to_string()
            }
        }
    }
}

struct BetSuccess {
    new_balance: i64,
    settlements: Vec<Settlement>,
    betting_countdown_id: Option<u64>,
    action_countdown_id: Option<u64>,
}

impl BlackjackService {
    pub fn new(
        chip_svc: ChipService,
        player_directory: BlackjackPlayerDirectory,
        event_tx: broadcast::Sender<BlackjackEvent>,
    ) -> Self {
        Self::new_with_settings(
            chip_svc,
            player_directory,
            event_tx,
            BlackjackTableSettings::default(),
        )
    }

    pub fn new_with_settings(
        chip_svc: ChipService,
        player_directory: BlackjackPlayerDirectory,
        event_tx: broadcast::Sender<BlackjackEvent>,
        settings: BlackjackTableSettings,
    ) -> Self {
        let table = SharedTableState::new(settings);
        let initial_snapshot = table.snapshot();
        let (snapshot_tx, snapshot_rx) = watch::channel(initial_snapshot);
        Self {
            chip_svc,
            player_directory,
            snapshot_tx,
            snapshot_rx,
            event_tx,
            table: Arc::new(Mutex::new(table)),
        }
    }

    pub fn subscribe_state(&self) -> watch::Receiver<BlackjackSnapshot> {
        self.snapshot_rx.clone()
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<BlackjackEvent> {
        self.event_tx.subscribe()
    }

    pub fn current_snapshot(&self) -> BlackjackSnapshot {
        self.snapshot_rx.borrow().clone()
    }

    pub fn sit_task(&self, user_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            match svc.sit(user_id).await {
                Ok(seat_index) => {
                    let _ = svc.event_tx.send(BlackjackEvent::SeatJoined {
                        user_id,
                        seat_index,
                    });
                }
                Err(failure) => {
                    let _ = svc.event_tx.send(BlackjackEvent::ActionError {
                        user_id,
                        message: failure.user_message(),
                    });
                }
            }
        });
    }

    async fn sit(&self, user_id: Uuid) -> Result<usize, SeatFailure> {
        let seat_index = {
            let mut table = self.table.lock().await;
            let seat_index = table.sit(user_id)?;
            table.status_message =
                format!("Seat {} joined. {}", seat_index + 1, table.betting_prompt());
            self.publish_snapshot_locked(&table);
            seat_index
        };

        if let Ok(player) = self.player_directory.player_info(user_id).await {
            let mut table = self.table.lock().await;
            table.set_player_info(user_id, player);
            self.publish_snapshot_locked(&table);
        }

        Ok(seat_index)
    }

    pub fn leave_seat_task(&self, user_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            match svc.leave_seat(user_id).await {
                Ok(seat_index) => {
                    let _ = svc.event_tx.send(BlackjackEvent::SeatLeft {
                        user_id,
                        seat_index,
                    });
                }
                Err(failure) => {
                    let _ = svc.event_tx.send(BlackjackEvent::ActionError {
                        user_id,
                        message: failure.user_message(),
                    });
                }
            }
        });
    }

    async fn leave_seat(&self, user_id: Uuid) -> Result<usize, SeatFailure> {
        let mut table = self.table.lock().await;
        let seat_index = table.leave_seat(user_id)?;
        self.publish_snapshot_locked(&table);
        Ok(seat_index)
    }

    pub fn throw_chip_task(&self, user_id: Uuid, chip: i64) {
        let svc = self.clone();
        tokio::spawn(async move {
            if let Err(failure) = svc.throw_chip(user_id, chip).await {
                let _ = svc.event_tx.send(BlackjackEvent::ActionError {
                    user_id,
                    message: failure.user_message(),
                });
            }
        });
    }

    async fn throw_chip(&self, user_id: Uuid, chip: i64) -> Result<(), StakeFailure> {
        let mut table = self.table.lock().await;
        table.throw_chip(user_id, chip)?;
        self.publish_snapshot_locked(&table);
        Ok(())
    }

    pub fn pull_stake_chip_task(&self, user_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            if let Err(failure) = svc.pull_stake_chip(user_id).await {
                let _ = svc.event_tx.send(BlackjackEvent::ActionError {
                    user_id,
                    message: failure.user_message(),
                });
            }
        });
    }

    async fn pull_stake_chip(&self, user_id: Uuid) -> Result<(), StakeFailure> {
        let mut table = self.table.lock().await;
        table.pull_stake_chip(user_id)?;
        self.publish_snapshot_locked(&table);
        Ok(())
    }

    pub fn clear_stake_task(&self, user_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            if let Err(failure) = svc.clear_stake(user_id).await {
                let _ = svc.event_tx.send(BlackjackEvent::ActionError {
                    user_id,
                    message: failure.user_message(),
                });
            }
        });
    }

    async fn clear_stake(&self, user_id: Uuid) -> Result<(), StakeFailure> {
        let mut table = self.table.lock().await;
        table.clear_stake(user_id)?;
        self.publish_snapshot_locked(&table);
        Ok(())
    }

    pub fn submit_stake_task(&self, user_id: Uuid, request_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            let result = match svc.submit_stake(user_id).await {
                Ok(success) => {
                    if let Some(countdown_id) = success.betting_countdown_id {
                        svc.schedule_auto_deal(countdown_id);
                    }
                    if let Some(countdown_id) = success.action_countdown_id {
                        svc.schedule_action_timeout(countdown_id);
                    }
                    if let Err(e) = svc.persist_settlements(success.settlements).await {
                        tracing::error!(error = ?e, %user_id, "blackjack submit_stake settlement failed");
                        Err("internal error".to_string())
                    } else {
                        Ok(success.new_balance)
                    }
                }
                Err(failure) => {
                    if let BetFailure::Internal(ref e) = failure {
                        tracing::error!(error = ?e, %user_id, "blackjack submit_stake failed");
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

    async fn submit_stake(&self, user_id: Uuid) -> Result<BetSuccess, BetFailure> {
        let amount = {
            let table = self.table.lock().await;
            let Some(seat_index) = table.user_seat_index(user_id) else {
                return Err(BetFailure::NotSeated);
            };
            if table.phase != Phase::Betting {
                return Err(BetFailure::TableBusy);
            }
            if table.seats[seat_index].bet.is_some()
                || table.seats[seat_index].pending_bet.is_some()
            {
                return Err(BetFailure::AlreadyBet);
            }
            let amount = table.seats[seat_index].stake_amount();
            if amount == 0 {
                return Err(BetFailure::NoStake);
            }
            amount
        };
        self.place_bet(user_id, amount).await
    }

    pub fn place_bet_task(&self, user_id: Uuid, request_id: Uuid, amount: i64) {
        let svc = self.clone();
        tokio::spawn(async move {
            let result = match svc.place_bet(user_id, amount).await {
                Ok(success) => {
                    if let Some(countdown_id) = success.betting_countdown_id {
                        svc.schedule_auto_deal(countdown_id);
                    }
                    if let Some(countdown_id) = success.action_countdown_id {
                        svc.schedule_action_timeout(countdown_id);
                    }
                    if let Err(e) = svc.persist_settlements(success.settlements).await {
                        tracing::error!(error = ?e, %user_id, amount, "blackjack place_bet settlement failed");
                        Err("internal error".to_string())
                    } else {
                        Ok(success.new_balance)
                    }
                }
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

    async fn place_bet(&self, user_id: Uuid, amount: i64) -> Result<BetSuccess, BetFailure> {
        {
            let mut table = self.table.lock().await;
            let Some(seat_index) = table.user_seat_index(user_id) else {
                return Err(BetFailure::NotSeated);
            };
            if table.phase != Phase::Betting {
                return Err(BetFailure::TableBusy);
            }
            if table.seats[seat_index].bet.is_some()
                || table.seats[seat_index].pending_bet.is_some()
            {
                return Err(BetFailure::AlreadyBet);
            }
            let bet = table.bet_for_amount(amount)?;
            table.seats[seat_index].pending_bet = Some(bet);
            table.seats[seat_index].stake_chips.clear();
            table.status_message = format!("Seat {} is placing {amount} chips...", seat_index + 1);
            self.publish_snapshot_locked(&table);
        }

        let new_balance = match self.chip_svc.debit_bet(user_id, amount).await {
            Ok(Some(new_balance)) => new_balance,
            Ok(None) => {
                let mut table = self.table.lock().await;
                if let Some(seat_index) = table.user_seat_index(user_id) {
                    table.seats[seat_index].pending_bet = None;
                }
                table.status_message = "insufficient chips".to_string();
                self.publish_snapshot_locked(&table);
                return Err(BetFailure::InsufficientChips);
            }
            Err(e) => {
                let mut table = self.table.lock().await;
                if let Some(seat_index) = table.user_seat_index(user_id) {
                    table.seats[seat_index].pending_bet = None;
                }
                table.status_message = "internal error".to_string();
                self.publish_snapshot_locked(&table);
                return Err(BetFailure::Internal(e));
            }
        };

        {
            let mut table = self.table.lock().await;
            if let Some(seat_index) = table.user_seat_index(user_id) {
                let bet = if let Some(bet) = table.seats[seat_index].pending_bet.take() {
                    bet
                } else {
                    table.bet_for_amount(amount)?
                };
                table.seats[seat_index].bet = Some(bet);
                table.update_player_balance(user_id, new_balance);

                let mut settlements = Vec::new();
                let mut action_countdown_id = None;
                let betting_countdown_id = if table.all_seated_bets_ready() {
                    settlements = table
                        .start_round()
                        .map_err(|e| BetFailure::Internal(anyhow::anyhow!("{e:?}")))?;
                    action_countdown_id = table.start_action_countdown_if_needed();
                    None
                } else {
                    Some(table.ensure_betting_countdown())
                };
                table.status_message = table.countdown_status();
                self.publish_snapshot_locked(&table);
                return Ok(BetSuccess {
                    new_balance,
                    settlements,
                    betting_countdown_id,
                    action_countdown_id,
                });
            }
            self.publish_snapshot_locked(&table);
        }

        Ok(BetSuccess {
            new_balance,
            settlements: Vec::new(),
            betting_countdown_id: None,
            action_countdown_id: None,
        })
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

    async fn hit(&self, user_id: Uuid) -> Result<(), ActionFailure> {
        let settlements = {
            let mut table = self.table.lock().await;
            let Some(seat_index) = table.user_seat_index(user_id) else {
                return Err(ActionFailure::NotSeated);
            };
            if table.phase != Phase::PlayerTurn {
                return Err(ActionFailure::InvalidPhase("you cannot hit right now"));
            }
            let settlements = table.hit_seat(seat_index)?;
            self.publish_snapshot_locked(&table);
            settlements
        };

        if !settlements.is_empty() {
            self.persist_settlements(settlements)
                .await
                .map_err(ActionFailure::Internal)?;
        }

        Ok(())
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

    async fn stand(&self, user_id: Uuid) -> Result<(), ActionFailure> {
        let settlements = {
            let mut table = self.table.lock().await;
            let Some(seat_index) = table.user_seat_index(user_id) else {
                return Err(ActionFailure::NotSeated);
            };
            if table.phase != Phase::PlayerTurn {
                return Err(ActionFailure::InvalidPhase("you cannot stand right now"));
            }
            let settlements = table.stand_seat(seat_index)?;
            self.publish_snapshot_locked(&table);
            settlements
        };

        if !settlements.is_empty() {
            self.persist_settlements(settlements)
                .await
                .map_err(ActionFailure::Internal)?;
        }

        Ok(())
    }

    pub fn deal_task(&self, user_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            match svc.deal(user_id).await {
                Ok(settlements) => {
                    let action_countdown_id = {
                        let mut table = svc.table.lock().await;
                        let action_countdown_id = table.start_action_countdown_if_needed();
                        svc.publish_snapshot_locked(&table);
                        action_countdown_id
                    };
                    if let Some(countdown_id) = action_countdown_id {
                        svc.schedule_action_timeout(countdown_id);
                    }
                    if let Err(e) = svc.persist_settlements(settlements).await {
                        tracing::error!(error = ?e, %user_id, "blackjack deal settlement failed");
                        let _ = svc.event_tx.send(BlackjackEvent::ActionError {
                            user_id,
                            message: "internal error".to_string(),
                        });
                    }
                }
                Err(failure) => {
                    if let ActionFailure::Internal(ref e) = failure {
                        tracing::error!(error = ?e, %user_id, "blackjack deal failed");
                    }
                    let _ = svc.event_tx.send(BlackjackEvent::ActionError {
                        user_id,
                        message: failure.user_message(),
                    });
                }
            }
        });
    }

    async fn deal(&self, user_id: Uuid) -> Result<Vec<Settlement>, ActionFailure> {
        let mut table = self.table.lock().await;
        if table.user_seat_index(user_id).is_none() {
            return Err(ActionFailure::NotSeated);
        }
        if table.phase != Phase::Betting {
            return Err(ActionFailure::InvalidPhase("hand is already in progress"));
        }
        let settlements = table.start_round()?;
        self.publish_snapshot_locked(&table);
        Ok(settlements)
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

    async fn next_hand(&self, user_id: Uuid) -> Result<(), ActionFailure> {
        let mut table = self.table.lock().await;
        if table.user_seat_index(user_id).is_none() {
            return Err(ActionFailure::NotSeated);
        }
        if table.phase != Phase::Settling {
            return Err(ActionFailure::InvalidPhase("hand is still in progress"));
        }
        let status = table.betting_prompt_with_timer();
        table.reset_to_betting(&status);
        self.publish_snapshot_locked(&table);
        Ok(())
    }

    fn schedule_auto_deal(&self, countdown_id: u64) {
        let svc = self.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(1)).await;

                let (settlements, action_countdown_id) = {
                    let mut table = svc.table.lock().await;
                    if !table.countdown_matches(countdown_id) {
                        return;
                    }

                    if table.has_pending_bets() && table.betting_countdown_secs() == Some(0) {
                        table.status_message =
                            "Waiting for pending bets before dealing.".to_string();
                        svc.publish_snapshot_locked(&table);
                        continue;
                    }

                    if table.betting_countdown_secs().is_some_and(|secs| secs > 0) {
                        table.status_message = table.betting_countdown_status();
                        svc.publish_snapshot_locked(&table);
                        continue;
                    }

                    match table.start_round_from_countdown(countdown_id) {
                        Ok(settlements) => {
                            let action_countdown_id = table.start_action_countdown_if_needed();
                            svc.publish_snapshot_locked(&table);
                            (settlements, action_countdown_id)
                        }
                        Err(failure) => {
                            table.clear_betting_countdown();
                            table.status_message = failure.user_message();
                            svc.publish_snapshot_locked(&table);
                            return;
                        }
                    }
                };

                if let Some(countdown_id) = action_countdown_id {
                    svc.schedule_action_timeout(countdown_id);
                }
                if let Err(e) = svc.persist_settlements(settlements).await {
                    tracing::error!(error = ?e, "blackjack auto-deal settlement failed");
                }
                return;
            }
        });
    }

    fn schedule_action_timeout(&self, countdown_id: u64) {
        let svc = self.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(1)).await;

                let settlements = {
                    let mut table = svc.table.lock().await;
                    if !table.action_countdown_matches(countdown_id) {
                        return;
                    }

                    if table.action_countdown_secs().is_some_and(|secs| secs > 0) {
                        svc.publish_snapshot_locked(&table);
                        continue;
                    }

                    let settlements = table.auto_stand_remaining();
                    svc.publish_snapshot_locked(&table);
                    settlements
                };

                if let Err(e) = svc.persist_settlements(settlements).await {
                    tracing::error!(error = ?e, "blackjack action-timeout settlement failed");
                }
                return;
            }
        });
    }

    async fn persist_settlements(&self, settlements: Vec<Settlement>) -> anyhow::Result<()> {
        for settlement in settlements {
            let new_balance = if settlement.credit == 0 {
                self.chip_svc.restore_floor(settlement.user_id).await?
            } else {
                self.chip_svc
                    .credit_payout(settlement.user_id, settlement.credit)
                    .await?
            };
            {
                let mut table = self.table.lock().await;
                table.update_player_balance(settlement.user_id, new_balance);
                self.publish_snapshot_locked(&table);
            }
            let _ = self.event_tx.send(BlackjackEvent::HandSettled {
                user_id: settlement.user_id,
                bet: settlement.bet,
                outcome: settlement.outcome,
                credit: settlement.credit,
                new_balance,
            });
        }
        Ok(())
    }

    fn publish_snapshot_locked(&self, table: &SharedTableState) {
        let _ = self.snapshot_tx.send(table.snapshot());
    }
}

struct SharedTableState {
    settings: BlackjackTableSettings,
    shoe: Shoe,
    seats: Vec<SeatState>,
    dealer_hand: Vec<PlayingCard>,
    phase: Phase,
    betting_deadline: Option<Instant>,
    betting_countdown_id: u64,
    action_deadline: Option<Instant>,
    action_countdown_id: u64,
    status_message: String,
}

#[derive(Clone, Debug)]
struct SeatState {
    user_id: Option<Uuid>,
    player: Option<BlackjackPlayerInfo>,
    stake_chips: Vec<i64>,
    pending_bet: Option<Bet>,
    bet: Option<Bet>,
    hand: Vec<PlayingCard>,
    stood: bool,
    last_outcome: Option<Outcome>,
    last_net_change: i64,
}

#[derive(Clone, Copy, Debug)]
struct Settlement {
    user_id: Uuid,
    bet: i64,
    outcome: Outcome,
    credit: i64,
}

impl SeatState {
    fn empty() -> Self {
        Self {
            user_id: None,
            player: None,
            stake_chips: Vec::new(),
            pending_bet: None,
            bet: None,
            hand: Vec::new(),
            stood: false,
            last_outcome: None,
            last_net_change: 0,
        }
    }

    fn snapshot(&self, index: usize, table_phase: Phase) -> BlackjackSeat {
        BlackjackSeat {
            index,
            user_id: self.user_id,
            player: self.player.clone(),
            bet_amount: self.bet.or(self.pending_bet).map(Bet::amount),
            stake_chips: self.stake_chips.clone(),
            hand: self.hand.clone(),
            phase: self.phase(table_phase),
            score: if self.hand.is_empty() {
                None
            } else {
                Some(score(&self.hand))
            },
            last_outcome: self.last_outcome,
            last_net_change: self.last_net_change,
        }
    }

    fn phase(&self, table_phase: Phase) -> SeatPhase {
        if self.user_id.is_none() {
            return SeatPhase::Empty;
        }
        if self.pending_bet.is_some() {
            return SeatPhase::BetPending;
        }
        if self.last_outcome.is_some() {
            return SeatPhase::Settled;
        }
        if self.stood {
            return SeatPhase::Stood;
        }
        if self.has_unresolved_bet() && table_phase == Phase::PlayerTurn {
            return SeatPhase::Playing;
        }
        if self.bet.is_some() {
            return SeatPhase::Ready;
        }
        SeatPhase::Seated
    }

    fn clear_round(&mut self) {
        self.pending_bet = None;
        self.bet = None;
        self.stake_chips.clear();
        self.hand.clear();
        self.stood = false;
        self.last_outcome = None;
        self.last_net_change = 0;
    }

    fn has_unresolved_bet(&self) -> bool {
        self.bet.is_some() && self.last_outcome.is_none()
    }

    fn stake_amount(&self) -> i64 {
        self.stake_chips.iter().sum()
    }
}

impl SharedTableState {
    fn new(settings: BlackjackTableSettings) -> Self {
        Self {
            settings: settings.normalized(),
            shoe: Shoe::new(),
            seats: vec![SeatState::empty(); MAX_SEATS],
            dealer_hand: Vec::new(),
            phase: Phase::Betting,
            betting_deadline: None,
            betting_countdown_id: 0,
            action_deadline: None,
            action_countdown_id: 0,
            status_message: "Sit to join, or watch the table.".to_string(),
        }
    }

    fn snapshot(&self) -> BlackjackSnapshot {
        BlackjackSnapshot {
            balance: 0,
            seats: self
                .seats
                .iter()
                .enumerate()
                .map(|(index, seat)| seat.snapshot(index, self.phase))
                .collect(),
            betting_countdown_secs: self.betting_countdown_secs(),
            action_countdown_secs: self.action_countdown_secs(),
            dealer_hand: self.dealer_hand.clone(),
            player_hand: self
                .reference_seat()
                .map_or_else(Vec::new, |seat| seat.hand.clone()),
            current_bet_amount: self
                .reference_seat()
                .and_then(|seat| seat.bet)
                .map(Bet::amount),
            min_bet: self.settings.min_bet(),
            max_bet: self.settings.max_bet(),
            chip_denominations: self.settings.chip_denominations(),
            phase: self.phase,
            last_outcome: self.reference_seat().and_then(|seat| seat.last_outcome),
            last_net_change: self.reference_seat().map_or(0, |seat| seat.last_net_change),
            stake_chips: Vec::new(),
            selected_chip_index: 0,
            status_message: self.status_message.clone(),
            dealer_revealed: matches!(self.phase, Phase::DealerTurn | Phase::Settling),
            dealer_score: if matches!(self.phase, Phase::DealerTurn | Phase::Settling) {
                Some(score(&self.dealer_hand))
            } else {
                None
            },
            player_score: self.reference_seat().and_then(|seat| {
                if seat.hand.is_empty() {
                    None
                } else {
                    Some(score(&seat.hand))
                }
            }),
        }
    }

    fn reference_seat(&self) -> Option<&SeatState> {
        self.seats
            .iter()
            .find(|seat| seat.has_unresolved_bet() && !seat.stood)
            .or_else(|| self.seats.iter().find(|seat| seat.bet.is_some()))
    }

    fn ensure_betting_countdown(&mut self) -> u64 {
        if self.betting_deadline.is_none() {
            self.betting_countdown_id = self.betting_countdown_id.wrapping_add(1);
            self.betting_deadline =
                Some(Instant::now() + Duration::from_secs(BETTING_LOCK_CAP_SECS));
        }
        self.betting_countdown_id
    }

    fn clear_betting_countdown(&mut self) {
        self.betting_deadline = None;
    }

    fn countdown_matches(&self, countdown_id: u64) -> bool {
        self.phase == Phase::Betting
            && self.betting_deadline.is_some()
            && self.betting_countdown_id == countdown_id
    }

    fn betting_countdown_secs(&self) -> Option<u64> {
        let deadline = self.betting_deadline?;
        let remaining = deadline.saturating_duration_since(Instant::now());
        let millis = remaining.as_millis() as u64;
        Some(millis.div_ceil(1000))
    }

    fn betting_countdown_status(&self) -> String {
        match self.betting_countdown_secs() {
            Some(0) => "Dealing now.".to_string(),
            Some(secs) => format!("Dealing in {secs}s unless all seated players lock."),
            None => self.betting_prompt(),
        }
    }

    fn start_action_countdown_if_needed(&mut self) -> Option<u64> {
        if self.phase != Phase::PlayerTurn {
            self.clear_action_countdown();
            return None;
        }
        self.action_countdown_id = self.action_countdown_id.wrapping_add(1);
        self.action_deadline =
            Some(Instant::now() + Duration::from_secs(self.settings.action_timeout_secs()));
        self.status_message = self.action_countdown_status();
        Some(self.action_countdown_id)
    }

    fn clear_action_countdown(&mut self) {
        self.action_deadline = None;
    }

    fn action_countdown_matches(&self, countdown_id: u64) -> bool {
        self.phase == Phase::PlayerTurn
            && self.action_deadline.is_some()
            && self.action_countdown_id == countdown_id
    }

    fn action_countdown_secs(&self) -> Option<u64> {
        let deadline = self.action_deadline?;
        let remaining = deadline.saturating_duration_since(Instant::now());
        let millis = remaining.as_millis() as u64;
        Some(millis.div_ceil(1000))
    }

    fn action_countdown_status(&self) -> String {
        match self.action_countdown_secs() {
            Some(0) => "Action timer expired. Standing remaining hands.".to_string(),
            Some(secs) => format!("Players hit or stand. Auto-stand in {secs}s."),
            None => "Players hit or stand.".to_string(),
        }
    }

    fn betting_prompt(&self) -> String {
        format!(
            "Place bets ({}-{} chips).",
            self.settings.min_bet(),
            self.settings.max_bet()
        )
    }

    fn betting_prompt_with_timer(&self) -> String {
        format!(
            "{} First locked bet starts a {BETTING_LOCK_CAP_SECS}s deal cap.",
            self.betting_prompt()
        )
    }

    fn countdown_status(&self) -> String {
        match self.phase {
            Phase::Betting => self.betting_countdown_status(),
            Phase::PlayerTurn => self.action_countdown_status(),
            _ => self.status_message.clone(),
        }
    }

    fn bet_for_amount(&self, amount: i64) -> Result<Bet, BetFailure> {
        Bet::new_for_table(amount, self.settings.min_bet(), self.settings.max_bet()).map_err(|e| {
            match e {
                crate::app::rooms::blackjack::state::BetError::BelowMin => {
                    BetFailure::BelowMin(self.settings.min_bet())
                }
                crate::app::rooms::blackjack::state::BetError::AboveMax => {
                    BetFailure::AboveMax(self.settings.max_bet())
                }
            }
        })
    }

    fn has_pending_bets(&self) -> bool {
        self.seats.iter().any(|seat| seat.pending_bet.is_some())
    }

    fn all_seated_bets_ready(&self) -> bool {
        let mut seated = 0;
        for seat in &self.seats {
            if seat.user_id.is_some() {
                seated += 1;
                if seat.bet.is_none() || seat.pending_bet.is_some() {
                    return false;
                }
            }
        }
        seated > 0
    }

    fn throw_chip(&mut self, user_id: Uuid, chip: i64) -> Result<(), StakeFailure> {
        let seat_index = self.stake_seat_index(user_id)?;
        if !self.settings.chip_denominations().contains(&chip) {
            return Err(StakeFailure::InvalidChip);
        }
        let next_amount = self.seats[seat_index].stake_amount() + chip;
        if next_amount > self.settings.max_bet() {
            return Err(StakeFailure::AboveMax(self.settings.max_bet()));
        }
        self.seats[seat_index].stake_chips.push(chip);
        self.status_message = format!("Seat {} is building a stake.", seat_index + 1);
        Ok(())
    }

    fn pull_stake_chip(&mut self, user_id: Uuid) -> Result<(), StakeFailure> {
        let seat_index = self.stake_seat_index(user_id)?;
        self.seats[seat_index].stake_chips.pop();
        self.status_message = self.betting_prompt_with_timer();
        Ok(())
    }

    fn clear_stake(&mut self, user_id: Uuid) -> Result<(), StakeFailure> {
        let seat_index = self.stake_seat_index(user_id)?;
        self.seats[seat_index].stake_chips.clear();
        self.status_message = self.betting_prompt_with_timer();
        Ok(())
    }

    fn stake_seat_index(&self, user_id: Uuid) -> Result<usize, StakeFailure> {
        let Some(seat_index) = self.user_seat_index(user_id) else {
            return Err(StakeFailure::NotSeated);
        };
        if self.phase != Phase::Betting {
            return Err(StakeFailure::TableBusy);
        }
        if self.seats[seat_index].bet.is_some() || self.seats[seat_index].pending_bet.is_some() {
            return Err(StakeFailure::AlreadyBet);
        }
        Ok(seat_index)
    }

    fn set_player_info(&mut self, user_id: Uuid, player: BlackjackPlayerInfo) {
        if let Some(seat) = self
            .seats
            .iter_mut()
            .find(|seat| seat.user_id == Some(user_id))
        {
            seat.player = Some(player);
        }
    }

    fn update_player_balance(&mut self, user_id: Uuid, balance: i64) {
        if let Some(seat) = self
            .seats
            .iter_mut()
            .find(|seat| seat.user_id == Some(user_id))
        {
            if let Some(player) = &mut seat.player {
                player.balance = balance;
            } else {
                seat.player = Some(BlackjackPlayerInfo {
                    user_id,
                    username: "player".to_string(),
                    balance,
                });
            }
        }
    }

    fn start_round_from_countdown(
        &mut self,
        countdown_id: u64,
    ) -> Result<Vec<Settlement>, ActionFailure> {
        if !self.countdown_matches(countdown_id) {
            return Err(ActionFailure::InvalidPhase("betting window changed"));
        }
        self.clear_betting_countdown();
        self.start_round()
    }

    fn start_round(&mut self) -> Result<Vec<Settlement>, ActionFailure> {
        self.clear_betting_countdown();
        self.clear_action_countdown();
        if self.seats.iter().any(|seat| seat.pending_bet.is_some()) {
            return Err(ActionFailure::InvalidPhase("wait for pending bets"));
        }
        if !self.seats.iter().any(|seat| seat.bet.is_some()) {
            return Err(ActionFailure::InvalidPhase("at least one bet is required"));
        }

        self.dealer_hand.clear();
        for seat in &mut self.seats {
            seat.stake_chips.clear();
            seat.hand.clear();
            seat.stood = false;
            seat.last_outcome = None;
            seat.last_net_change = 0;
        }

        for _ in 0..2 {
            for seat in &mut self.seats {
                if seat.bet.is_some() {
                    seat.hand.push(self.shoe.draw());
                }
            }
            self.dealer_hand.push(self.shoe.draw());
        }

        let dealer_blackjack = is_natural_blackjack(&self.dealer_hand);
        let mut settlements = Vec::new();
        for index in 0..self.seats.len() {
            if self.seats[index].bet.is_none() {
                continue;
            }
            let player_blackjack = is_natural_blackjack(&self.seats[index].hand);
            if player_blackjack || dealer_blackjack {
                let outcome = settle(&self.seats[index].hand, &self.dealer_hand);
                if let Some(settlement) = self.finish_seat(index, outcome) {
                    settlements.push(settlement);
                }
            }
        }

        if self.has_playable_seats() {
            self.phase = Phase::PlayerTurn;
            self.status_message = "Players hit or stand.".to_string();
        } else {
            self.phase = Phase::Settling;
            self.clear_action_countdown();
            self.status_message = "Round settled. Press Space or Enter for next hand.".to_string();
        }
        Ok(settlements)
    }

    fn hit_seat(&mut self, index: usize) -> Result<Vec<Settlement>, ActionFailure> {
        if !self.seats[index].has_unresolved_bet() || self.seats[index].stood {
            return Err(ActionFailure::InvalidPhase("your hand is not active"));
        }
        self.seats[index].hand.push(self.shoe.draw());
        let settlements = if is_bust(&self.seats[index].hand) {
            let mut settlements = Vec::new();
            if let Some(settlement) = self.finish_seat(index, Outcome::DealerWin) {
                settlements.push(settlement);
            }
            settlements.extend(self.advance_or_finish_round());
            settlements
        } else {
            self.status_message = format!(
                "Seat {} total: {}.",
                index + 1,
                score(&self.seats[index].hand).total
            );
            Vec::new()
        };
        Ok(settlements)
    }

    fn stand_seat(&mut self, index: usize) -> Result<Vec<Settlement>, ActionFailure> {
        if !self.seats[index].has_unresolved_bet() || self.seats[index].stood {
            return Err(ActionFailure::InvalidPhase("your hand is not active"));
        }
        self.seats[index].stood = true;
        Ok(self.advance_or_finish_round())
    }

    fn advance_or_finish_round(&mut self) -> Vec<Settlement> {
        if self.has_playable_seats() {
            self.phase = Phase::PlayerTurn;
            self.status_message = "Waiting for remaining seats.".to_string();
            return Vec::new();
        }

        self.clear_action_countdown();
        self.phase = Phase::DealerTurn;
        self.status_message = "Dealer's turn.".to_string();
        while dealer_must_hit(&self.dealer_hand) {
            self.dealer_hand.push(self.shoe.draw());
        }

        let mut settlements = Vec::new();
        for index in 0..self.seats.len() {
            if self.seats[index].has_unresolved_bet() {
                let outcome = settle(&self.seats[index].hand, &self.dealer_hand);
                if let Some(settlement) = self.finish_seat(index, outcome) {
                    settlements.push(settlement);
                }
            }
        }
        self.phase = Phase::Settling;
        self.status_message = "Round settled. Press Space or Enter for next hand.".to_string();
        settlements
    }

    fn auto_stand_remaining(&mut self) -> Vec<Settlement> {
        for seat in &mut self.seats {
            if seat.has_unresolved_bet() && !seat.stood {
                seat.stood = true;
            }
        }
        self.advance_or_finish_round()
    }

    fn has_playable_seats(&self) -> bool {
        self.seats
            .iter()
            .any(|seat| seat.has_unresolved_bet() && !seat.stood)
    }

    fn finish_seat(&mut self, index: usize, outcome: Outcome) -> Option<Settlement> {
        let seat = &mut self.seats[index];
        let bet = seat.bet?;
        let user_id = seat.user_id?;
        let credit = payout_credit(bet, outcome);
        seat.last_outcome = Some(outcome);
        seat.last_net_change = credit - bet.amount();
        seat.stood = false;
        Some(Settlement {
            user_id,
            bet: bet.amount(),
            outcome,
            credit,
        })
    }

    fn reset_to_betting(&mut self, status: &str) {
        self.dealer_hand.clear();
        self.phase = Phase::Betting;
        self.clear_betting_countdown();
        self.clear_action_countdown();
        for seat in &mut self.seats {
            seat.clear_round();
        }
        self.status_message = status.to_string();
    }

    fn sit(&mut self, user_id: Uuid) -> Result<usize, SeatFailure> {
        if self.user_seat_index(user_id).is_some() {
            return Err(SeatFailure::AlreadySeated);
        }
        let Some(seat_index) = self.seats.iter().position(|seat| seat.user_id.is_none()) else {
            return Err(SeatFailure::TableFull);
        };
        self.seats[seat_index].user_id = Some(user_id);
        Ok(seat_index)
    }

    fn leave_seat(&mut self, user_id: Uuid) -> Result<usize, SeatFailure> {
        let Some(seat_index) = self.user_seat_index(user_id) else {
            return Err(SeatFailure::NotSeated);
        };
        if !matches!(self.phase, Phase::Settling)
            && (self.seats[seat_index].bet.is_some()
                || self.seats[seat_index].pending_bet.is_some())
        {
            return Err(SeatFailure::CannotLeaveWithBet);
        }

        self.seats[seat_index] = SeatState::empty();
        self.status_message = format!("Seat {} left the table.", seat_index + 1);
        Ok(seat_index)
    }

    fn user_seat_index(&self, user_id: Uuid) -> Option<usize> {
        self.seats
            .iter()
            .position(|seat| seat.user_id == Some(user_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::rooms::blackjack::state::MIN_BET;

    fn user_id() -> Uuid {
        Uuid::now_v7()
    }

    #[test]
    fn seats_allow_four_players() {
        let mut table = SharedTableState::new(BlackjackTableSettings::default());
        let users = (0..=MAX_SEATS).map(|_| user_id()).collect::<Vec<_>>();

        for (index, user_id) in users.iter().take(MAX_SEATS).enumerate() {
            assert_eq!(table.sit(*user_id).expect("seat should be open"), index);
        }

        assert!(matches!(
            table.sit(users[MAX_SEATS]),
            Err(SeatFailure::TableFull)
        ));
    }

    #[test]
    fn same_user_cannot_take_two_seats() {
        let mut table = SharedTableState::new(BlackjackTableSettings::default());
        let user_id = user_id();

        assert_eq!(table.sit(user_id).expect("seat should be open"), 0);
        assert!(matches!(
            table.sit(user_id),
            Err(SeatFailure::AlreadySeated)
        ));
    }

    #[test]
    fn betting_seat_cannot_leave_mid_hand() {
        let mut table = SharedTableState::new(BlackjackTableSettings::default());
        let user_id = user_id();
        let seat_index = table.sit(user_id).expect("seat should be open");
        table.seats[seat_index].bet = Some(Bet::new(MIN_BET).unwrap());
        table.phase = Phase::PlayerTurn;

        assert!(matches!(
            table.leave_seat(user_id),
            Err(SeatFailure::CannotLeaveWithBet)
        ));
        assert_eq!(table.user_seat_index(user_id), Some(0));
    }

    #[test]
    fn betting_seat_can_leave_after_settlement() {
        let mut table = SharedTableState::new(BlackjackTableSettings::default());
        let user_id = user_id();
        let seat_index = table.sit(user_id).expect("seat should be open");
        table.seats[seat_index].bet = Some(Bet::new(MIN_BET).unwrap());
        table.seats[seat_index].last_outcome = Some(Outcome::Push);
        table.phase = Phase::Settling;

        assert_eq!(table.leave_seat(user_id).expect("leave should work"), 0);
        assert_eq!(table.user_seat_index(user_id), None);
        assert_eq!(table.phase, Phase::Settling);
    }

    #[test]
    fn deal_requires_at_least_one_bet() {
        let mut table = SharedTableState::new(BlackjackTableSettings::default());
        table.sit(user_id()).expect("seat should be open");

        assert!(matches!(
            table.start_round(),
            Err(ActionFailure::InvalidPhase("at least one bet is required"))
        ));
    }

    #[test]
    fn round_deals_each_betting_seat() {
        let mut table = SharedTableState::new(BlackjackTableSettings::default());
        let user_a = user_id();
        let user_b = user_id();
        let seat_a = table.sit(user_a).expect("seat should be open");
        let seat_b = table.sit(user_b).expect("seat should be open");
        table.seats[seat_a].bet = Some(Bet::new(MIN_BET).unwrap());
        table.seats[seat_b].bet = Some(Bet::new(MIN_BET).unwrap());

        let _ = table.start_round().expect("round should start");

        assert_eq!(table.dealer_hand.len(), 2);
        assert_eq!(table.seats[seat_a].hand.len(), 2);
        assert_eq!(table.seats[seat_b].hand.len(), 2);
        assert!(matches!(table.phase, Phase::PlayerTurn | Phase::Settling));
    }

    #[test]
    fn stand_waits_for_other_unresolved_seats() {
        let mut table = SharedTableState::new(BlackjackTableSettings::default());
        let user_a = user_id();
        let user_b = user_id();
        let seat_a = table.sit(user_a).expect("seat should be open");
        let seat_b = table.sit(user_b).expect("seat should be open");
        table.seats[seat_a].bet = Some(Bet::new(MIN_BET).unwrap());
        table.seats[seat_b].bet = Some(Bet::new(MIN_BET).unwrap());
        table.phase = Phase::PlayerTurn;

        let settlements = table.stand_seat(seat_a).expect("seat can stand");

        assert!(settlements.is_empty());
        assert!(table.seats[seat_a].stood);
        assert!(!table.seats[seat_b].stood);
        assert_eq!(table.phase, Phase::PlayerTurn);
    }

    #[test]
    fn betting_countdown_starts_once_as_hard_cap() {
        let mut table = SharedTableState::new(BlackjackTableSettings::default());

        let first_id = table.ensure_betting_countdown();
        let first_deadline = table.betting_deadline.expect("deadline should be set");
        let second_id = table.ensure_betting_countdown();
        let second_deadline = table.betting_deadline.expect("deadline should be set");

        assert_eq!(first_id, second_id);
        assert_eq!(second_deadline, first_deadline);
        assert!(table.countdown_matches(second_id));
    }

    #[test]
    fn all_seated_bets_ready_when_every_seated_player_has_locked_bet() {
        let mut table = SharedTableState::new(BlackjackTableSettings::default());
        let user_a = user_id();
        let user_b = user_id();
        let seat_a = table.sit(user_a).expect("seat should be open");
        let seat_b = table.sit(user_b).expect("seat should be open");

        table.seats[seat_a].bet = Some(Bet::new(MIN_BET).unwrap());
        assert!(!table.all_seated_bets_ready());

        table.seats[seat_b].bet = Some(Bet::new(MIN_BET).unwrap());
        assert!(table.all_seated_bets_ready());
    }

    #[test]
    fn thrown_stake_chips_are_visible_on_seat_snapshot() {
        let mut table = SharedTableState::new(BlackjackTableSettings::default());
        let user_id = user_id();
        let seat_index = table.sit(user_id).expect("seat should be open");

        table
            .throw_chip(user_id, MIN_BET)
            .expect("chip should be accepted");

        let snapshot = table.snapshot();
        assert_eq!(snapshot.seats[seat_index].stake_chips, vec![MIN_BET]);
        assert_eq!(snapshot.seats[seat_index].bet_amount, None);
    }
}
