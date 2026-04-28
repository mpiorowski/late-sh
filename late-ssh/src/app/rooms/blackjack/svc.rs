use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use tokio::sync::{Mutex, broadcast, watch};
use uuid::Uuid;

use crate::app::{
    games::{cards::PlayingCard, chips::svc::ChipService},
    rooms::blackjack::state::{
        Bet, BetError, BlackjackSeat, BlackjackSnapshot, MAX_BET, MAX_SEATS, MIN_BET, Outcome,
        Phase, SeatPhase, Shoe, dealer_must_hit, is_bust, is_natural_blackjack, payout_credit,
        score, seat_outcome_banner, settle,
    },
};

const BETTING_WINDOW: Duration = Duration::from_secs(5);

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
    BelowMin,
    AboveMax,
    NotSeated,
    AlreadyBet,
    TableBusy,
    InsufficientChips,
    Internal(anyhow::Error),
}

impl BetFailure {
    fn user_message(&self) -> String {
        match self {
            BetFailure::BelowMin => format!("bet below minimum ({MIN_BET})"),
            BetFailure::AboveMax => format!("bet above maximum ({MAX_BET})"),
            BetFailure::NotSeated => "sit before betting".to_string(),
            BetFailure::AlreadyBet => "bet already placed".to_string(),
            BetFailure::TableBusy => "table is busy".to_string(),
            BetFailure::InsufficientChips => "insufficient chips".to_string(),
            BetFailure::Internal(_) => "internal error".to_string(),
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

    pub fn deal_task(&self, user_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            match svc.deal(user_id).await {
                Ok(settlements) => {
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

    fn schedule_auto_deal(&self, countdown_id: u64) {
        let svc = self.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(1)).await;

                let settlements = {
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
                            svc.publish_snapshot_locked(&table);
                            settlements
                        }
                        Err(failure) => {
                            table.clear_betting_countdown();
                            table.status_message = failure.user_message();
                            svc.publish_snapshot_locked(&table);
                            return;
                        }
                    }
                };

                if let Err(e) = svc.persist_settlements(settlements).await {
                    tracing::error!(error = ?e, "blackjack auto-deal settlement failed");
                }
                return;
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

    async fn sit(&self, user_id: Uuid) -> Result<usize, SeatFailure> {
        let mut table = self.table.lock().await;
        let seat_index = table.sit(user_id)?;
        table.status_message = format!(
            "Seat {} joined. Place a bet ({MIN_BET}-{MAX_BET} chips).",
            seat_index + 1
        );
        self.publish_snapshot_locked(&table);
        Ok(seat_index)
    }

    async fn leave_seat(&self, user_id: Uuid) -> Result<usize, SeatFailure> {
        let mut table = self.table.lock().await;
        let seat_index = table.leave_seat(user_id)?;
        self.publish_snapshot_locked(&table);
        Ok(seat_index)
    }

    async fn place_bet(&self, user_id: Uuid, amount: i64) -> Result<i64, BetFailure> {
        Bet::new(amount).map_err(|e| match e {
            BetError::BelowMin => BetFailure::BelowMin,
            BetError::AboveMax => BetFailure::AboveMax,
        })?;

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
            let bet = Bet::new(amount).expect("validated bet");
            table.seats[seat_index].pending_bet = Some(bet);
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
                let bet = table.seats[seat_index]
                    .pending_bet
                    .take()
                    .unwrap_or_else(|| Bet::new(amount).expect("validated bet"));
                table.seats[seat_index].bet = Some(bet);
                let countdown_id = table.restart_betting_countdown();
                table.status_message = table.betting_countdown_status();
                self.publish_snapshot_locked(&table);
                drop(table);
                self.schedule_auto_deal(countdown_id);
                return Ok(new_balance);
            }
            self.publish_snapshot_locked(&table);
        }

        Ok(new_balance)
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

    async fn next_hand(&self, user_id: Uuid) -> Result<(), ActionFailure> {
        let mut table = self.table.lock().await;
        if table.user_seat_index(user_id).is_none() {
            return Err(ActionFailure::NotSeated);
        }
        if table.phase != Phase::Settling {
            return Err(ActionFailure::InvalidPhase("hand is still in progress"));
        }
        table.reset_to_betting(&format!(
            "Place bets ({MIN_BET}-{MAX_BET} chips). Each bet restarts the 5s deal timer."
        ));
        self.publish_snapshot_locked(&table);
        Ok(())
    }

    async fn persist_settlements(&self, settlements: Vec<Settlement>) -> anyhow::Result<()> {
        for settlement in settlements {
            let new_balance = if settlement.credit == 0 {
                None
            } else {
                Some(
                    self.chip_svc
                        .credit_payout(settlement.user_id, settlement.credit)
                        .await?,
                )
            };
            if let Some(new_balance) = new_balance {
                let _ = self.event_tx.send(BlackjackEvent::HandSettled {
                    user_id: settlement.user_id,
                    bet: settlement.bet,
                    outcome: settlement.outcome,
                    credit: settlement.credit,
                    new_balance,
                });
            }
        }
        Ok(())
    }

    fn publish_snapshot_locked(&self, table: &SharedTableState) {
        let _ = self.snapshot_tx.send(table.snapshot());
    }
}

struct SharedTableState {
    shoe: Shoe,
    seats: Vec<SeatState>,
    dealer_hand: Vec<PlayingCard>,
    phase: Phase,
    betting_deadline: Option<Instant>,
    betting_countdown_id: u64,
    status_message: String,
}

#[derive(Clone, Debug)]
struct SeatState {
    user_id: Option<Uuid>,
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
            bet_amount: self.bet.or(self.pending_bet).map(Bet::amount),
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
        self.hand.clear();
        self.stood = false;
        self.last_outcome = None;
        self.last_net_change = 0;
    }

    fn has_unresolved_bet(&self) -> bool {
        self.bet.is_some() && self.last_outcome.is_none()
    }
}

impl SharedTableState {
    fn new() -> Self {
        Self {
            shoe: Shoe::new(),
            seats: vec![SeatState::empty(); MAX_SEATS],
            dealer_hand: Vec::new(),
            phase: Phase::Betting,
            betting_deadline: None,
            betting_countdown_id: 0,
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
            dealer_hand: self.dealer_hand.clone(),
            player_hand: self
                .reference_seat()
                .map_or_else(Vec::new, |seat| seat.hand.clone()),
            current_bet_amount: self
                .reference_seat()
                .and_then(|seat| seat.bet)
                .map(Bet::amount),
            phase: self.phase,
            last_outcome: self.reference_seat().and_then(|seat| seat.last_outcome),
            last_net_change: self.reference_seat().map_or(0, |seat| seat.last_net_change),
            bet_input: String::new(),
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
            outcome_banner: self
                .reference_seat()
                .map(|seat| seat.snapshot(0, self.phase))
                .and_then(|seat| seat_outcome_banner(&seat)),
        }
    }

    fn reference_seat(&self) -> Option<&SeatState> {
        self.seats
            .iter()
            .find(|seat| seat.has_unresolved_bet() && !seat.stood)
            .or_else(|| self.seats.iter().find(|seat| seat.bet.is_some()))
    }

    fn restart_betting_countdown(&mut self) -> u64 {
        self.betting_countdown_id = self.betting_countdown_id.wrapping_add(1);
        self.betting_deadline = Some(Instant::now() + BETTING_WINDOW);
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
            Some(secs) => format!("Dealing in {secs}s. More seats can still bet."),
            None => format!("Place bets ({MIN_BET}-{MAX_BET} chips)."),
        }
    }

    fn has_pending_bets(&self) -> bool {
        self.seats.iter().any(|seat| seat.pending_bet.is_some())
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
        if self.seats.iter().any(|seat| seat.pending_bet.is_some()) {
            return Err(ActionFailure::InvalidPhase("wait for pending bets"));
        }
        if !self.seats.iter().any(|seat| seat.bet.is_some()) {
            return Err(ActionFailure::InvalidPhase("at least one bet is required"));
        }

        self.dealer_hand.clear();
        for seat in &mut self.seats {
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
            self.status_message = "Round settled. Press n or Enter for next hand.".to_string();
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
        self.status_message = "Round settled. Press n or Enter for next hand.".to_string();
        settlements
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

    fn user_id() -> Uuid {
        Uuid::now_v7()
    }

    #[test]
    fn seats_allow_four_players() {
        let mut table = SharedTableState::new();
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
        let mut table = SharedTableState::new();
        let user_id = user_id();

        assert_eq!(table.sit(user_id).expect("seat should be open"), 0);
        assert!(matches!(
            table.sit(user_id),
            Err(SeatFailure::AlreadySeated)
        ));
    }

    #[test]
    fn betting_seat_cannot_leave_mid_hand() {
        let mut table = SharedTableState::new();
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
        let mut table = SharedTableState::new();
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
        let mut table = SharedTableState::new();
        table.sit(user_id()).expect("seat should be open");

        assert!(matches!(
            table.start_round(),
            Err(ActionFailure::InvalidPhase("at least one bet is required"))
        ));
    }

    #[test]
    fn round_deals_each_betting_seat() {
        let mut table = SharedTableState::new();
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
        let mut table = SharedTableState::new();
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
    fn each_confirmed_bet_restarts_betting_countdown() {
        let mut table = SharedTableState::new();

        let first_id = table.restart_betting_countdown();
        let first_deadline = table.betting_deadline.expect("deadline should be set");
        let second_id = table.restart_betting_countdown();
        let second_deadline = table.betting_deadline.expect("deadline should be set");

        assert_ne!(first_id, second_id);
        assert!(second_deadline >= first_deadline);
        assert!(!table.countdown_matches(first_id));
        assert!(table.countdown_matches(second_id));
    }
}
