use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex as StdMutex},
    time::{Duration, Instant},
};

use late_core::MutexRecover;
use rand_core::{OsRng, RngCore};
use tokio::sync::{Mutex, watch};
use uuid::Uuid;

use crate::app::{
    activity::{event::ActivityGame, publisher::ActivityPublisher},
    games::{
        cards::{CardRank, CardSuit, PlayingCard},
        chips::svc::ChipService,
    },
};

pub const MAX_SEATS: usize = 4;
pub const SMALL_BLIND: i64 = 10;
pub const BIG_BLIND: i64 = 20;

const SEAT_IDLE_TIMEOUT_SECS: u64 = 5 * 60;

#[derive(Clone)]
pub struct PokerService {
    room_id: Uuid,
    chip_svc: ChipService,
    activity: ActivityPublisher,
    public_tx: watch::Sender<PokerPublicSnapshot>,
    public_rx: watch::Receiver<PokerPublicSnapshot>,
    private_txs: Arc<StdMutex<HashMap<Uuid, watch::Sender<PokerPrivateSnapshot>>>>,
    state: Arc<Mutex<SharedState>>,
}

#[derive(Clone, Debug)]
pub struct PokerPublicSnapshot {
    pub room_id: Uuid,
    pub seats: Vec<PokerSeat>,
    pub community: Vec<PlayingCard>,
    pub dealer_button: Option<usize>,
    pub active_seat: Option<usize>,
    pub phase: PokerPhase,
    pub hand_number: u64,
    pub winners: Vec<usize>,
    pub winning_rank: Option<String>,
    pub status_message: String,
    pub pot: i64,
    pub current_bet: i64,
    pub min_raise: i64,
    pub small_blind: i64,
    pub big_blind: i64,
    pub settlement_pending: bool,
}

#[derive(Clone, Debug)]
pub struct PokerSeat {
    pub index: usize,
    pub user_id: Option<Uuid>,
    pub card_count: usize,
    pub revealed_cards: Option<Vec<PlayingCard>>,
    pub folded: bool,
    pub in_hand: bool,
    pub last_action: Option<PokerAction>,
    pub balance: i64,
    pub committed: i64,
    pub street_bet: i64,
    pub all_in: bool,
    pub pending: bool,
}

#[derive(Clone, Debug, Default)]
pub struct PokerPrivateSnapshot {
    pub hole_cards: Vec<PlayingCard>,
    pub notice: Option<String>,
    pub balance: Option<i64>,
    pub to_call: i64,
    pub min_raise: i64,
    pub can_raise: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PokerPhase {
    Waiting,
    PostingBlinds,
    PreFlop,
    Flop,
    Turn,
    River,
    Showdown,
}

impl PokerPhase {
    pub fn label(self) -> &'static str {
        match self {
            Self::Waiting => "Waiting",
            Self::PostingBlinds => "Blinds",
            Self::PreFlop => "Pre-Flop",
            Self::Flop => "Flop",
            Self::Turn => "Turn",
            Self::River => "River",
            Self::Showdown => "Showdown",
        }
    }

    fn is_action_phase(self) -> bool {
        matches!(self, Self::PreFlop | Self::Flop | Self::Turn | Self::River)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PokerAction {
    SmallBlind,
    BigBlind,
    Check,
    Call,
    Bet,
    Raise,
    Fold,
    AllIn,
}

impl PokerAction {
    pub fn label(self) -> &'static str {
        match self {
            Self::SmallBlind => "Small blind",
            Self::BigBlind => "Big blind",
            Self::Check => "Check",
            Self::Call => "Call",
            Self::Bet => "Bet",
            Self::Raise => "Raise",
            Self::Fold => "Fold",
            Self::AllIn => "All-in",
        }
    }
}

impl PokerService {
    pub fn new(room_id: Uuid, chip_svc: ChipService, activity: ActivityPublisher) -> Self {
        let state = SharedState::new(room_id);
        let initial_snapshot = state.public_snapshot();
        let (public_tx, public_rx) = watch::channel(initial_snapshot);
        Self {
            room_id,
            chip_svc,
            activity,
            public_tx,
            public_rx,
            private_txs: Arc::new(StdMutex::new(HashMap::new())),
            state: Arc::new(Mutex::new(state)),
        }
    }

    pub fn room_id(&self) -> Uuid {
        self.room_id
    }

    pub fn subscribe_public(&self) -> watch::Receiver<PokerPublicSnapshot> {
        self.public_rx.clone()
    }

    pub fn subscribe_private(&self, user_id: Uuid) -> watch::Receiver<PokerPrivateSnapshot> {
        let mut private_txs = self.private_txs.lock_recover();
        if let Some(tx) = private_txs.get(&user_id) {
            return tx.subscribe();
        }

        let (tx, rx) = watch::channel(PokerPrivateSnapshot::default());
        private_txs.insert(user_id, tx.clone());
        drop(private_txs);

        let svc = self.clone();
        tokio::spawn(async move {
            let state = svc.state.lock().await;
            svc.publish_private_to(&state, user_id, &tx);
        });

        rx
    }

    pub fn current_snapshot(&self) -> PokerPublicSnapshot {
        self.public_rx.borrow().clone()
    }

    pub fn sit_task(&self, user_id: Uuid, balance: i64) {
        let svc = self.clone();
        tokio::spawn(async move {
            let activity_generation = {
                let mut state = svc.state.lock().await;
                state.sit(user_id, balance);
                let activity_generation = state.record_activity(user_id);
                svc.publish(&state);
                activity_generation
            };
            if let Some(activity_generation) = activity_generation {
                svc.schedule_inactivity_kick(user_id, activity_generation);
            }
        });
    }

    pub fn leave_seat_task(&self, user_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            let settlements = {
                let mut state = svc.state.lock().await;
                let settlements = state.leave(user_id);
                svc.publish(&state);
                settlements
            };
            if !settlements.is_empty() {
                svc.persist_settlements_task(settlements);
            }
        });
    }

    pub fn start_hand_task(&self, user_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            let (activity_generation, requests) = {
                let mut state = svc.state.lock().await;
                let requests = state.start_hand(user_id);
                let activity_generation = state.record_activity(user_id);
                svc.publish(&state);
                (activity_generation, requests)
            };
            if let Some(activity_generation) = activity_generation {
                svc.schedule_inactivity_kick(user_id, activity_generation);
            }
            for request in requests {
                svc.commit_request_task(request);
            }
        });
    }

    pub fn call_or_check_task(&self, user_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            let (activity_generation, outcome) = {
                let mut state = svc.state.lock().await;
                let outcome = state.call_or_check(user_id);
                let activity_generation = state.record_activity(user_id);
                svc.publish(&state);
                (activity_generation, outcome)
            };
            if let Some(activity_generation) = activity_generation {
                svc.schedule_inactivity_kick(user_id, activity_generation);
            }
            svc.handle_action_outcome(outcome);
        });
    }

    pub fn bet_or_raise_task(&self, user_id: Uuid, raise_by: i64) {
        let svc = self.clone();
        tokio::spawn(async move {
            let (activity_generation, outcome) = {
                let mut state = svc.state.lock().await;
                let outcome = state.bet_or_raise(user_id, raise_by);
                let activity_generation = state.record_activity(user_id);
                svc.publish(&state);
                (activity_generation, outcome)
            };
            if let Some(activity_generation) = activity_generation {
                svc.schedule_inactivity_kick(user_id, activity_generation);
            }
            svc.handle_action_outcome(outcome);
        });
    }

    pub fn all_in_task(&self, user_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            let (activity_generation, outcome) = {
                let mut state = svc.state.lock().await;
                let outcome = state.all_in(user_id);
                let activity_generation = state.record_activity(user_id);
                svc.publish(&state);
                (activity_generation, outcome)
            };
            if let Some(activity_generation) = activity_generation {
                svc.schedule_inactivity_kick(user_id, activity_generation);
            }
            svc.handle_action_outcome(outcome);
        });
    }

    pub fn fold_task(&self, user_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            let (activity_generation, settlements) = {
                let mut state = svc.state.lock().await;
                let settlements = state.fold(user_id);
                let activity_generation = state.record_activity(user_id);
                svc.publish(&state);
                (activity_generation, settlements)
            };
            if let Some(activity_generation) = activity_generation {
                svc.schedule_inactivity_kick(user_id, activity_generation);
            }
            if !settlements.is_empty() {
                svc.persist_settlements_task(settlements);
            }
        });
    }

    pub fn sync_balance_task(&self, user_id: Uuid, balance: i64) {
        let svc = self.clone();
        tokio::spawn(async move {
            let mut state = svc.state.lock().await;
            let state_changed = state.sync_balance(user_id, balance);
            if state_changed {
                svc.publish(&state);
            }
        });
    }

    pub fn touch_activity_task(&self, user_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            let activity_generation = {
                let mut state = svc.state.lock().await;
                state.record_activity(user_id)
            };
            if let Some(activity_generation) = activity_generation {
                svc.schedule_inactivity_kick(user_id, activity_generation);
            }
        });
    }

    fn handle_action_outcome(&self, outcome: ActionOutcome) {
        match outcome {
            ActionOutcome::None => {}
            ActionOutcome::Commit(request) => self.commit_request_task(request),
            ActionOutcome::Settlements(settlements) => {
                if !settlements.is_empty() {
                    self.persist_settlements_task(settlements);
                }
            }
        }
    }

    fn commit_request_task(&self, request: CommitRequest) {
        let svc = self.clone();
        tokio::spawn(async move {
            let result = svc
                .chip_svc
                .debit_bet(request.user_id, request.amount)
                .await;
            let settlements = {
                let mut state = svc.state.lock().await;
                let settlements = match result {
                    Ok(Some(new_balance)) => state.apply_commit_success(request, new_balance),
                    Ok(None) => state.apply_commit_failure(
                        request,
                        "Not enough chips available for that action.".to_string(),
                    ),
                    Err(e) => {
                        tracing::error!(
                            error = ?e,
                            user_id = %request.user_id,
                            amount = request.amount,
                            "poker chip debit failed"
                        );
                        state.apply_commit_failure(
                            request,
                            "Chip debit failed. Try again.".to_string(),
                        )
                    }
                };
                svc.publish(&state);
                settlements
            };
            if !settlements.is_empty() {
                svc.persist_settlements_task(settlements);
            }
        });
    }

    fn persist_settlements_task(&self, settlements: Vec<PokerSettlement>) {
        let svc = self.clone();
        tokio::spawn(async move {
            let mut updates = Vec::with_capacity(settlements.len());
            let mut failed = false;
            for settlement in settlements {
                let result = if settlement.credit == 0 {
                    svc.chip_svc.restore_floor(settlement.user_id).await
                } else {
                    svc.chip_svc
                        .credit_payout(settlement.user_id, settlement.credit)
                        .await
                };

                match result {
                    Ok(new_balance) => {
                        if settlement.credit > 0 {
                            svc.activity.game_won_task(
                                settlement.user_id,
                                ActivityGame::Poker,
                                Some(format!("pot {}", settlement.credit)),
                                None,
                            );
                        }
                        updates.push((settlement.user_id, new_balance));
                    }
                    Err(e) => {
                        failed = true;
                        tracing::error!(
                            error = ?e,
                            user_id = %settlement.user_id,
                            credit = settlement.credit,
                            "poker settlement failed"
                        );
                    }
                }
            }

            let mut state = svc.state.lock().await;
            if failed {
                state.settlement_failed();
            } else {
                state.complete_settlements(updates);
            }
            svc.publish(&state);
        });
    }

    fn schedule_inactivity_kick(&self, user_id: Uuid, activity_generation: u64) {
        let svc = self.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(SEAT_IDLE_TIMEOUT_SECS)).await;

            let (changed, settlements) = {
                let mut state = svc.state.lock().await;
                let (changed, settlements) = state.kick_inactive_user(user_id, activity_generation);
                if changed {
                    svc.publish(&state);
                }
                (changed, settlements)
            };
            if changed && !settlements.is_empty() {
                svc.persist_settlements_task(settlements);
            }
        });
    }

    fn publish(&self, state: &SharedState) {
        let _ = self.public_tx.send(state.public_snapshot());

        let mut private_txs = self.private_txs.lock_recover();
        private_txs.retain(|_, tx| tx.receiver_count() > 0);
        for (user_id, tx) in private_txs.iter() {
            self.publish_private_to(state, *user_id, tx);
        }
    }

    fn publish_private_to(
        &self,
        state: &SharedState,
        user_id: Uuid,
        tx: &watch::Sender<PokerPrivateSnapshot>,
    ) {
        let _ = tx.send(state.private_snapshot_for(user_id));
    }
}

struct SharedState {
    room_id: Uuid,
    seats: [Option<Uuid>; MAX_SEATS],
    balances: [i64; MAX_SEATS],
    hole_cards: [Vec<PlayingCard>; MAX_SEATS],
    folded: [bool; MAX_SEATS],
    all_in: [bool; MAX_SEATS],
    acted_this_street: [bool; MAX_SEATS],
    last_action: [Option<PokerAction>; MAX_SEATS],
    leave_after_hand: [bool; MAX_SEATS],
    committed: [i64; MAX_SEATS],
    street_bet: [i64; MAX_SEATS],
    pending_commit: [Option<PendingCommit>; MAX_SEATS],
    community: Vec<PlayingCard>,
    deck: Vec<PlayingCard>,
    dealer_button: Option<usize>,
    small_blind_seat: Option<usize>,
    big_blind_seat: Option<usize>,
    active_seat: Option<usize>,
    phase: PokerPhase,
    hand_number: u64,
    winners: Vec<usize>,
    winning_rank: Option<String>,
    status_message: String,
    current_bet: i64,
    min_raise: i64,
    pending_blinds: usize,
    settlement_pending: bool,
    showdown_reveals: bool,
    last_balances: HashMap<Uuid, i64>,
    last_activity: [Instant; MAX_SEATS],
    activity_generation: [u64; MAX_SEATS],
    next_request_id: u64,
}

impl SharedState {
    fn new(room_id: Uuid) -> Self {
        let now = Instant::now();
        Self {
            room_id,
            seats: [None; MAX_SEATS],
            balances: [0; MAX_SEATS],
            hole_cards: std::array::from_fn(|_| Vec::new()),
            folded: [false; MAX_SEATS],
            all_in: [false; MAX_SEATS],
            acted_this_street: [false; MAX_SEATS],
            last_action: [None; MAX_SEATS],
            leave_after_hand: [false; MAX_SEATS],
            committed: [0; MAX_SEATS],
            street_bet: [0; MAX_SEATS],
            pending_commit: [None; MAX_SEATS],
            community: Vec::new(),
            deck: Vec::new(),
            dealer_button: None,
            small_blind_seat: None,
            big_blind_seat: None,
            active_seat: None,
            phase: PokerPhase::Waiting,
            hand_number: 0,
            winners: Vec::new(),
            winning_rank: None,
            status_message: "Take a seat. Two players can deal a hand.".to_string(),
            current_bet: 0,
            min_raise: BIG_BLIND,
            pending_blinds: 0,
            settlement_pending: false,
            showdown_reveals: false,
            last_balances: HashMap::new(),
            last_activity: [now; MAX_SEATS],
            activity_generation: [0; MAX_SEATS],
            next_request_id: 0,
        }
    }

    fn public_snapshot(&self) -> PokerPublicSnapshot {
        PokerPublicSnapshot {
            room_id: self.room_id,
            seats: (0..MAX_SEATS)
                .map(|index| self.seat_snapshot(index))
                .collect(),
            community: self.community.clone(),
            dealer_button: self.dealer_button,
            active_seat: self.active_seat,
            phase: self.phase,
            hand_number: self.hand_number,
            winners: self.winners.clone(),
            winning_rank: self.winning_rank.clone(),
            status_message: self.status_message.clone(),
            pot: self.pot(),
            current_bet: self.current_bet,
            min_raise: self.min_raise,
            small_blind: SMALL_BLIND,
            big_blind: BIG_BLIND,
            settlement_pending: self.settlement_pending,
        }
    }

    fn private_snapshot_for(&self, user_id: Uuid) -> PokerPrivateSnapshot {
        let seat_index = self.seat_index(user_id);
        let hole_cards = seat_index
            .map(|index| self.hole_cards[index].clone())
            .unwrap_or_default();
        let notice = if !hole_cards.is_empty() && self.showdown_reveals {
            Some("Showdown cards are public.".to_string())
        } else if !hole_cards.is_empty() {
            Some("Your hole cards are private.".to_string())
        } else {
            None
        };
        let balance = seat_index
            .map(|index| self.balances[index])
            .or_else(|| self.last_balances.get(&user_id).copied());
        let to_call = seat_index
            .map(|index| self.to_call(index))
            .unwrap_or_default();
        let can_raise = seat_index
            .map(|index| self.can_raise(index))
            .unwrap_or_default();

        PokerPrivateSnapshot {
            hole_cards,
            notice,
            balance,
            to_call,
            min_raise: self.min_raise,
            can_raise,
        }
    }

    fn seat_snapshot(&self, index: usize) -> PokerSeat {
        let card_count = self.hole_cards[index].len();
        let revealed_cards = self.revealed_cards_for(index);
        PokerSeat {
            index,
            user_id: self.seats[index],
            card_count,
            revealed_cards,
            folded: card_count > 0 && self.folded[index],
            in_hand: card_count > 0 && self.seats[index].is_some(),
            last_action: self.last_action[index],
            balance: self.balances[index],
            committed: self.committed[index],
            street_bet: self.street_bet[index],
            all_in: self.all_in[index],
            pending: self.pending_commit[index].is_some(),
        }
    }

    fn revealed_cards_for(&self, index: usize) -> Option<Vec<PlayingCard>> {
        if self.phase != PokerPhase::Showdown
            || !self.showdown_reveals
            || self.seats[index].is_none()
            || self.folded[index]
            || self.hole_cards[index].len() != 2
        {
            return None;
        }
        Some(self.hole_cards[index].clone())
    }

    fn sit(&mut self, user_id: Uuid, balance: i64) {
        if let Some(index) = self.seat_index(user_id) {
            self.sync_balance_at(index, user_id, balance);
            return;
        }
        if self.phase.is_action_phase() || self.phase == PokerPhase::PostingBlinds {
            self.status_message = "Wait for the hand to finish before sitting.".to_string();
            return;
        }
        let Some(index) = self.seats.iter().position(Option::is_none) else {
            self.status_message = "Poker table is full.".to_string();
            return;
        };
        self.seats[index] = Some(user_id);
        self.sync_balance_at(index, user_id, balance);
        self.status_message = if self.playable_indices().len() >= 2 {
            "Press n to deal a hand.".to_string()
        } else {
            "Waiting for a second funded player.".to_string()
        };
    }

    fn sync_balance(&mut self, user_id: Uuid, balance: i64) -> bool {
        self.last_balances.insert(user_id, balance.max(0));
        let Some(index) = self.seat_index(user_id) else {
            return false;
        };
        if !self.can_sync_seat_balance(index) {
            return false;
        }
        self.sync_balance_at(index, user_id, balance);
        true
    }

    fn sync_balance_at(&mut self, index: usize, user_id: Uuid, balance: i64) {
        let balance = balance.max(0);
        self.balances[index] = balance;
        self.last_balances.insert(user_id, balance);
    }

    fn can_sync_seat_balance(&self, index: usize) -> bool {
        !self.settlement_pending
            && !self.phase.is_action_phase()
            && self.phase != PokerPhase::PostingBlinds
            && self.pending_commit[index].is_none()
    }

    fn leave(&mut self, user_id: Uuid) -> Vec<PokerSettlement> {
        let Some(index) = self.seat_index(user_id) else {
            return Vec::new();
        };
        if self.pending_commit[index].is_some() || self.settlement_pending {
            self.status_message = "Wait for pending chips to settle before leaving.".to_string();
            return Vec::new();
        }
        if self.phase == PokerPhase::PostingBlinds {
            self.status_message = "Blinds are posting. Wait a moment before leaving.".to_string();
            return Vec::new();
        }
        if self.phase.is_action_phase() && self.hole_cards[index].len() == 2 {
            self.folded[index] = true;
            self.acted_this_street[index] = true;
            self.last_action[index] = Some(PokerAction::Fold);
            self.leave_after_hand[index] = true;
            if self.active_seat == Some(index) {
                return self.advance_after_action(index);
            }
            if self.active_player_indices().len() == 1 {
                return self.finish_by_fold(self.active_player_indices()[0]);
            }
            self.status_message = format!("Seat {} folded and will sit out.", index + 1);
            return Vec::new();
        }

        self.remove_seat(index);
        self.status_message = if self.occupied_count() == 0 {
            "Take a seat. Two players can deal a hand.".to_string()
        } else {
            "Seat left the table.".to_string()
        };
        Vec::new()
    }

    fn start_hand(&mut self, user_id: Uuid) -> Vec<CommitRequest> {
        if self.seat_index(user_id).is_none() {
            self.status_message = "Sit before dealing a hand.".to_string();
            return Vec::new();
        }
        if self.settlement_pending {
            self.status_message = "Settling the previous pot.".to_string();
            return Vec::new();
        }
        if !matches!(self.phase, PokerPhase::Waiting | PokerPhase::Showdown) {
            self.status_message = "Finish the current hand first.".to_string();
            return Vec::new();
        }
        if self.pending_commit.iter().any(Option::is_some) {
            self.status_message = "Wait for pending chip actions.".to_string();
            return Vec::new();
        }

        let playable = self.playable_indices();
        if playable.len() < 2 {
            self.status_message = "Need at least two funded players to deal.".to_string();
            return Vec::new();
        }

        self.deck = fresh_deck();
        shuffle(&mut self.deck);
        self.community.clear();
        self.hole_cards = std::array::from_fn(|_| Vec::new());
        self.folded = [false; MAX_SEATS];
        self.all_in = [false; MAX_SEATS];
        self.acted_this_street = [false; MAX_SEATS];
        self.last_action = [None; MAX_SEATS];
        self.leave_after_hand = [false; MAX_SEATS];
        self.committed = [0; MAX_SEATS];
        self.street_bet = [0; MAX_SEATS];
        self.pending_commit = [None; MAX_SEATS];
        self.winners.clear();
        self.winning_rank = None;
        self.current_bet = 0;
        self.min_raise = BIG_BLIND;
        self.pending_blinds = 0;
        self.settlement_pending = false;
        self.showdown_reveals = false;
        self.active_seat = None;

        let dealer = self
            .dealer_button
            .and_then(|index| self.next_playable_after(index))
            .or_else(|| {
                self.seat_index(user_id)
                    .filter(|index| self.balances[*index] > 0)
            })
            .or_else(|| playable.first().copied())
            .unwrap_or(0);
        self.dealer_button = Some(dealer);

        let playable = self.playable_indices();
        for _ in 0..2 {
            for index in &playable {
                if let Some(card) = self.deck.pop() {
                    self.hole_cards[*index].push(card);
                }
            }
        }

        let (small_blind, big_blind) = self.blind_seats_for(dealer);
        self.small_blind_seat = Some(small_blind);
        self.big_blind_seat = Some(big_blind);
        self.phase = PokerPhase::PostingBlinds;
        self.hand_number = self.hand_number.saturating_add(1);

        let mut requests = Vec::new();
        if let Some(request) =
            self.prepare_forced_commit(small_blind, CommitKind::SmallBlind, SMALL_BLIND)
        {
            requests.push(request);
        }
        if big_blind != small_blind
            && let Some(request) =
                self.prepare_forced_commit(big_blind, CommitKind::BigBlind, BIG_BLIND)
        {
            requests.push(request);
        }
        self.pending_blinds = requests.len();
        self.status_message = format!(
            "Hand {} dealt. Posting {}/{} blinds.",
            self.hand_number, SMALL_BLIND, BIG_BLIND
        );

        requests
    }

    fn prepare_forced_commit(
        &mut self,
        index: usize,
        kind: CommitKind,
        blind: i64,
    ) -> Option<CommitRequest> {
        if self.seats[index].is_none() || self.hole_cards[index].len() != 2 {
            return None;
        }
        let amount = blind.min(self.balances[index]).max(0);
        if amount == 0 {
            self.folded[index] = true;
            self.last_action[index] = Some(PokerAction::Fold);
            return None;
        }
        let target_bet = self.street_bet[index] + amount;
        Some(self.set_pending_commit(index, kind, amount, target_bet, target_bet))
    }

    fn call_or_check(&mut self, user_id: Uuid) -> ActionOutcome {
        let Some(index) = self.validate_active_user(user_id) else {
            return ActionOutcome::None;
        };
        let to_call = self.to_call(index);
        if to_call == 0 {
            self.acted_this_street[index] = true;
            self.last_action[index] = Some(PokerAction::Check);
            return ActionOutcome::Settlements(self.advance_after_action(index));
        }

        let amount = to_call.min(self.balances[index]);
        if amount <= 0 {
            self.status_message = "No chips available to call.".to_string();
            return ActionOutcome::None;
        }
        let target_bet = self.street_bet[index] + amount;
        let kind = if amount == self.balances[index] {
            CommitKind::AllIn
        } else {
            CommitKind::Call
        };
        ActionOutcome::Commit(self.set_pending_commit(index, kind, amount, target_bet, 0))
    }

    fn bet_or_raise(&mut self, user_id: Uuid, raise_by: i64) -> ActionOutcome {
        let Some(index) = self.validate_active_user(user_id) else {
            return ActionOutcome::None;
        };
        if !self.can_raise(index) {
            self.status_message =
                "A short all-in did not reopen raising. Call or fold.".to_string();
            return ActionOutcome::None;
        }
        if self.balances[index] <= 0 {
            self.status_message = "No chips available to bet.".to_string();
            return ActionOutcome::None;
        }

        let raise_by = raise_by.max(self.min_raise).max(BIG_BLIND);
        let max_target = self.street_bet[index] + self.balances[index];
        let target_bet = if self.current_bet == 0 {
            raise_by.min(max_target)
        } else {
            (self.current_bet + raise_by).min(max_target)
        };
        if target_bet <= self.current_bet && target_bet < max_target {
            self.status_message = format!("Raise must be at least {}.", self.min_raise);
            return ActionOutcome::None;
        }

        let amount = target_bet - self.street_bet[index];
        if amount <= 0 {
            self.status_message = "Nothing to bet.".to_string();
            return ActionOutcome::None;
        }
        let raise_size = (target_bet - self.current_bet).max(0);
        let kind = if amount == self.balances[index] {
            CommitKind::AllIn
        } else {
            CommitKind::BetRaise
        };
        ActionOutcome::Commit(self.set_pending_commit(index, kind, amount, target_bet, raise_size))
    }

    fn all_in(&mut self, user_id: Uuid) -> ActionOutcome {
        let Some(index) = self.validate_active_user(user_id) else {
            return ActionOutcome::None;
        };
        let amount = self.balances[index];
        if amount <= 0 {
            self.status_message = "No chips available to shove.".to_string();
            return ActionOutcome::None;
        }
        let target_bet = self.street_bet[index] + amount;
        if target_bet > self.current_bet && !self.can_raise(index) {
            self.status_message =
                "A short all-in did not reopen raising. Call or fold.".to_string();
            return ActionOutcome::None;
        }
        let raise_size = (target_bet - self.current_bet).max(0);
        ActionOutcome::Commit(self.set_pending_commit(
            index,
            CommitKind::AllIn,
            amount,
            target_bet,
            raise_size,
        ))
    }

    fn fold(&mut self, user_id: Uuid) -> Vec<PokerSettlement> {
        let Some(index) = self.validate_active_user(user_id) else {
            return Vec::new();
        };
        self.folded[index] = true;
        self.acted_this_street[index] = true;
        self.last_action[index] = Some(PokerAction::Fold);
        self.advance_after_action(index)
    }

    fn validate_active_user(&mut self, user_id: Uuid) -> Option<usize> {
        if !self.phase.is_action_phase() {
            self.status_message = "No poker action is pending.".to_string();
            return None;
        }
        let Some(index) = self.seat_index(user_id) else {
            self.status_message = "Sit before playing.".to_string();
            return None;
        };
        if self.pending_commit[index].is_some() {
            self.status_message = "Waiting for chip debit.".to_string();
            return None;
        }
        if self.active_seat != Some(index) {
            self.status_message = match self.active_seat {
                Some(active) => format!("Seat {} acts now.", active + 1),
                None => "No active player.".to_string(),
            };
            return None;
        }
        if self.hole_cards[index].len() != 2 || self.folded[index] || self.all_in[index] {
            self.status_message = "You are not in this action.".to_string();
            return None;
        }
        Some(index)
    }

    fn set_pending_commit(
        &mut self,
        index: usize,
        kind: CommitKind,
        amount: i64,
        target_bet: i64,
        raise_size: i64,
    ) -> CommitRequest {
        self.next_request_id = self.next_request_id.wrapping_add(1);
        let request = CommitRequest {
            request_id: self.next_request_id,
            user_id: self.seats[index].expect("pending commit requires seated user"),
            seat_index: index,
            amount,
        };
        self.pending_commit[index] = Some(PendingCommit {
            request_id: request.request_id,
            kind,
            amount,
            target_bet,
            raise_size,
        });
        self.status_message = format!("Seat {} committing {} chips.", index + 1, amount);
        request
    }

    fn apply_commit_success(
        &mut self,
        request: CommitRequest,
        new_balance: i64,
    ) -> Vec<PokerSettlement> {
        if self.pending_commit[request.seat_index]
            .is_none_or(|pending| pending.request_id != request.request_id)
        {
            return Vec::new();
        }
        let Some(pending) = self.pending_commit[request.seat_index].take() else {
            return Vec::new();
        };
        let index = request.seat_index;
        let Some(user_id) = self.seats[index] else {
            return Vec::new();
        };

        self.balances[index] = new_balance.max(0);
        self.last_balances.insert(user_id, self.balances[index]);
        self.committed[index] += pending.amount;
        self.street_bet[index] += pending.amount;
        if self.balances[index] == 0 {
            self.all_in[index] = true;
        }

        let old_current_bet = self.current_bet;
        let was_raise = pending.target_bet > old_current_bet;
        if was_raise {
            let raise_size = pending.raise_size.max(pending.target_bet - old_current_bet);
            let full_raise = raise_size >= self.min_raise;
            self.current_bet = pending.target_bet;
            if full_raise {
                self.min_raise = raise_size;
            }
            if full_raise && self.phase.is_action_phase() {
                self.reset_other_action_flags(index);
            }
        }

        self.acted_this_street[index] =
            !matches!(pending.kind, CommitKind::SmallBlind | CommitKind::BigBlind);
        self.last_action[index] = Some(self.action_for_commit(pending.kind, old_current_bet));

        if self.phase == PokerPhase::PostingBlinds {
            self.pending_blinds = self.pending_blinds.saturating_sub(1);
            if self.pending_blinds == 0 {
                return self.finish_blind_posting();
            }
            self.status_message = format!("Posted blind for seat {}.", index + 1);
            return Vec::new();
        }

        self.advance_after_action(index)
    }

    fn apply_commit_failure(
        &mut self,
        request: CommitRequest,
        message: String,
    ) -> Vec<PokerSettlement> {
        if self.pending_commit[request.seat_index]
            .is_none_or(|pending| pending.request_id != request.request_id)
        {
            return Vec::new();
        }
        self.pending_commit[request.seat_index] = None;

        if self.phase == PokerPhase::PostingBlinds {
            self.folded[request.seat_index] = true;
            self.last_action[request.seat_index] = Some(PokerAction::Fold);
            self.pending_blinds = self.pending_blinds.saturating_sub(1);
            self.status_message = format!(
                "Seat {} could not post a blind and folded.",
                request.seat_index + 1
            );
            if self.pending_blinds == 0 {
                return self.finish_blind_posting();
            }
            return Vec::new();
        }

        self.status_message = message;
        Vec::new()
    }

    fn action_for_commit(&self, kind: CommitKind, old_current_bet: i64) -> PokerAction {
        match kind {
            CommitKind::SmallBlind => PokerAction::SmallBlind,
            CommitKind::BigBlind => PokerAction::BigBlind,
            CommitKind::Call => PokerAction::Call,
            CommitKind::BetRaise if old_current_bet == 0 => PokerAction::Bet,
            CommitKind::BetRaise => PokerAction::Raise,
            CommitKind::AllIn => PokerAction::AllIn,
        }
    }

    fn reset_other_action_flags(&mut self, actor: usize) {
        for index in 0..MAX_SEATS {
            if index != actor && self.hole_cards[index].len() == 2 && !self.folded[index] {
                self.acted_this_street[index] = false;
            }
        }
    }

    fn finish_blind_posting(&mut self) -> Vec<PokerSettlement> {
        self.current_bet = self.street_bet.iter().copied().max().unwrap_or_default();
        self.min_raise = BIG_BLIND;
        let active_players = self.active_player_indices();
        if active_players.len() == 1 {
            return self.finish_by_fold(active_players[0]);
        }
        if active_players.is_empty() {
            self.phase = PokerPhase::Waiting;
            self.active_seat = None;
            self.status_message = "Hand ended before action.".to_string();
            return Vec::new();
        }

        self.phase = PokerPhase::PreFlop;
        let start = self.big_blind_seat.or(self.dealer_button).unwrap_or(0);
        self.active_seat = self.next_action_after(start);
        if self.active_seat.is_none() && self.street_complete() {
            self.deal_remaining_community();
            return self.finish_showdown();
        }
        self.status_message = match self.active_seat {
            Some(index) => format!("Blinds posted. Seat {} acts.", index + 1),
            None => "Blinds posted.".to_string(),
        };
        Vec::new()
    }

    fn advance_after_action(&mut self, acted_index: usize) -> Vec<PokerSettlement> {
        let active_players = self.active_player_indices();
        if active_players.len() == 1 {
            return self.finish_by_fold(active_players[0]);
        }
        if active_players.is_empty() {
            self.phase = PokerPhase::Waiting;
            self.active_seat = None;
            self.status_message = "Hand ended. Waiting for players.".to_string();
            return Vec::new();
        }

        if self.street_complete() {
            if self.should_runout_to_showdown() {
                self.deal_remaining_community();
                return self.finish_showdown();
            }
            return self.advance_street();
        }

        self.active_seat = self.next_action_after(acted_index);
        self.status_message = match self.active_seat {
            Some(index) => format!("Seat {} acts.", index + 1),
            None => "Waiting for action.".to_string(),
        };
        Vec::new()
    }

    fn advance_street(&mut self) -> Vec<PokerSettlement> {
        self.acted_this_street = [false; MAX_SEATS];
        self.street_bet = [0; MAX_SEATS];
        self.current_bet = 0;
        self.min_raise = BIG_BLIND;

        match self.phase {
            PokerPhase::PreFlop => {
                self.deal_community(3);
                self.phase = PokerPhase::Flop;
                self.set_first_action_for_new_street("Flop dealt")
            }
            PokerPhase::Flop => {
                self.deal_community(1);
                self.phase = PokerPhase::Turn;
                self.set_first_action_for_new_street("Turn dealt")
            }
            PokerPhase::Turn => {
                self.deal_community(1);
                self.phase = PokerPhase::River;
                self.set_first_action_for_new_street("River dealt")
            }
            PokerPhase::River => self.finish_showdown(),
            _ => Vec::new(),
        }
    }

    fn set_first_action_for_new_street(&mut self, prefix: &'static str) -> Vec<PokerSettlement> {
        if self.should_runout_to_showdown() {
            self.deal_remaining_community();
            return self.finish_showdown();
        }

        let dealer = self.dealer_button.unwrap_or(0);
        self.active_seat = self.next_action_after(dealer);
        self.status_message = match self.active_seat {
            Some(index) => format!("{prefix}. Seat {} acts.", index + 1),
            None => format!("{prefix}."),
        };
        Vec::new()
    }

    fn finish_by_fold(&mut self, winner: usize) -> Vec<PokerSettlement> {
        self.phase = PokerPhase::Showdown;
        self.active_seat = None;
        self.winners = vec![winner];
        self.winning_rank = None;
        self.showdown_reveals = false;

        let settlements = self.settlements_for_single_winner(winner);
        self.settlement_pending = !settlements.is_empty();
        self.status_message = if settlements.is_empty() {
            format!("Seat {} wins by fold. Press n for next hand.", winner + 1)
        } else {
            format!(
                "Seat {} wins {} by fold. Settling pot.",
                winner + 1,
                self.pot()
            )
        };
        settlements
    }

    fn finish_showdown(&mut self) -> Vec<PokerSettlement> {
        let contenders = self.active_player_indices();
        if contenders.is_empty() {
            self.phase = PokerPhase::Waiting;
            self.active_seat = None;
            self.status_message = "No contenders remain.".to_string();
            return Vec::new();
        }

        let awards = self.calculate_pot_awards();
        let mut winners = HashSet::new();
        for award in &awards {
            for winner in &award.winners {
                winners.insert(*winner);
            }
        }
        self.winners = winners.into_iter().collect();
        self.winners.sort_unstable();

        self.winning_rank = contenders
            .iter()
            .map(|index| self.evaluate_seat(*index))
            .max_by_key(|hand| hand.value)
            .map(|hand| hand.label.to_string());
        self.phase = PokerPhase::Showdown;
        self.active_seat = None;
        self.showdown_reveals = true;

        let settlements = self.settlements_from_awards(&awards);
        self.settlement_pending = !settlements.is_empty();

        let winners = seat_list(&self.winners);
        let rank = self
            .winning_rank
            .as_deref()
            .unwrap_or("best hand")
            .to_string();
        self.status_message = if settlements.is_empty() {
            format!("{winners} win with {rank}. Press n for next hand.")
        } else {
            format!("{winners} win {} with {rank}. Settling pot.", self.pot())
        };
        settlements
    }

    fn calculate_pot_awards(&self) -> Vec<PotAward> {
        let side_pots = self.side_pots();
        let mut awards = Vec::with_capacity(side_pots.len());

        for pot in side_pots {
            if pot.eligible.is_empty() || pot.amount <= 0 {
                continue;
            }
            let mut scored = Vec::with_capacity(pot.eligible.len());
            for index in pot.eligible {
                scored.push((index, self.evaluate_seat(index)));
            }
            let Some(best) = scored.iter().map(|(_, hand)| hand.value).max() else {
                continue;
            };
            let winners = scored
                .iter()
                .filter_map(|(index, hand)| (hand.value == best).then_some(*index))
                .collect::<Vec<_>>();
            awards.push(PotAward {
                amount: pot.amount,
                winners,
            });
        }

        awards
    }

    fn side_pots(&self) -> Vec<SidePot> {
        let mut levels = self
            .committed
            .iter()
            .copied()
            .filter(|amount| *amount > 0)
            .collect::<Vec<_>>();
        levels.sort_unstable();
        levels.dedup();

        let mut pots = Vec::new();
        let mut previous = 0;
        for level in levels {
            let contributors = (0..MAX_SEATS)
                .filter(|index| self.committed[*index] >= level)
                .collect::<Vec<_>>();
            let amount = (level - previous) * contributors.len() as i64;
            let eligible = contributors
                .iter()
                .copied()
                .filter(|index| {
                    self.seats[*index].is_some()
                        && self.hole_cards[*index].len() == 2
                        && !self.folded[*index]
                })
                .collect::<Vec<_>>();
            pots.push(SidePot { amount, eligible });
            previous = level;
        }
        pots
    }

    fn settlements_from_awards(&self, awards: &[PotAward]) -> Vec<PokerSettlement> {
        let mut credits = [0; MAX_SEATS];
        for award in awards {
            if award.amount <= 0 || award.winners.is_empty() {
                continue;
            }
            let mut winners = award.winners.clone();
            winners.sort_unstable();
            let share = award.amount / winners.len() as i64;
            let remainder = award.amount % winners.len() as i64;
            for (offset, winner) in winners.into_iter().enumerate() {
                let odd_chip = if (offset as i64) < remainder { 1 } else { 0 };
                credits[winner] += share + odd_chip;
            }
        }

        (0..MAX_SEATS)
            .filter_map(|index| {
                let user_id = self.seats[index]?;
                (self.committed[index] > 0).then_some(PokerSettlement {
                    user_id,
                    credit: credits[index],
                })
            })
            .collect()
    }

    fn settlements_for_single_winner(&self, winner: usize) -> Vec<PokerSettlement> {
        let pot = self.pot();
        (0..MAX_SEATS)
            .filter_map(|index| {
                let user_id = self.seats[index]?;
                (self.committed[index] > 0).then_some(PokerSettlement {
                    user_id,
                    credit: if index == winner { pot } else { 0 },
                })
            })
            .collect()
    }

    fn complete_settlements(&mut self, updates: Vec<(Uuid, i64)>) {
        for (user_id, balance) in updates {
            self.last_balances.insert(user_id, balance);
            if let Some(index) = self.seat_index(user_id) {
                self.balances[index] = balance;
            }
        }
        for index in 0..MAX_SEATS {
            if self.leave_after_hand[index] {
                self.remove_seat(index);
            }
        }
        self.settlement_pending = false;
        self.status_message = if self.winners.is_empty() {
            "Pot settled. Press n for next hand.".to_string()
        } else {
            format!(
                "{} settled. Press n for next hand.",
                seat_list(&self.winners)
            )
        };
    }

    fn settlement_failed(&mut self) {
        self.settlement_pending = false;
        self.status_message = "Poker settlement failed. Check logs before continuing.".to_string();
    }

    fn evaluate_seat(&self, index: usize) -> EvaluatedHand {
        let mut cards = self.hole_cards[index].clone();
        cards.extend(self.community.iter().copied());
        evaluate_best_hand(&cards)
    }

    fn deal_community(&mut self, count: usize) {
        for _ in 0..count {
            if let Some(card) = self.deck.pop() {
                self.community.push(card);
            }
        }
    }

    fn deal_remaining_community(&mut self) {
        while self.community.len() < 5 {
            self.deal_community(1);
        }
    }

    fn street_complete(&self) -> bool {
        let active_players = self.active_player_indices();
        !active_players.is_empty()
            && active_players.into_iter().all(|index| {
                self.all_in[index]
                    || (self.acted_this_street[index] && self.street_bet[index] >= self.current_bet)
            })
    }

    fn should_runout_to_showdown(&self) -> bool {
        self.active_player_indices().len() > 1 && self.actionable_player_indices().len() <= 1
    }

    fn pot(&self) -> i64 {
        self.committed.iter().sum()
    }

    fn to_call(&self, index: usize) -> i64 {
        self.current_bet.saturating_sub(self.street_bet[index])
    }

    fn can_raise(&self, index: usize) -> bool {
        self.phase.is_action_phase()
            && self.seats[index].is_some()
            && self.hole_cards[index].len() == 2
            && !self.folded[index]
            && !self.all_in[index]
            && !self.acted_this_street[index]
    }

    fn occupied_count(&self) -> usize {
        self.seats.iter().filter(|seat| seat.is_some()).count()
    }

    fn playable_indices(&self) -> Vec<usize> {
        self.seats
            .iter()
            .enumerate()
            .filter_map(|(index, seat)| {
                (seat.is_some() && self.balances[index] > 0).then_some(index)
            })
            .collect()
    }

    fn active_player_indices(&self) -> Vec<usize> {
        (0..MAX_SEATS)
            .filter(|index| {
                self.seats[*index].is_some()
                    && self.hole_cards[*index].len() == 2
                    && !self.folded[*index]
            })
            .collect()
    }

    fn actionable_player_indices(&self) -> Vec<usize> {
        self.active_player_indices()
            .into_iter()
            .filter(|index| !self.all_in[*index])
            .collect()
    }

    fn next_playable_after(&self, start: usize) -> Option<usize> {
        (1..=MAX_SEATS)
            .map(|offset| (start + offset) % MAX_SEATS)
            .find(|index| self.seats[*index].is_some() && self.balances[*index] > 0)
    }

    fn next_action_after(&self, start: usize) -> Option<usize> {
        (1..=MAX_SEATS)
            .map(|offset| (start + offset) % MAX_SEATS)
            .find(|index| {
                self.seats[*index].is_some()
                    && self.hole_cards[*index].len() == 2
                    && !self.folded[*index]
                    && !self.all_in[*index]
            })
    }

    fn blind_seats_for(&self, dealer: usize) -> (usize, usize) {
        let occupied = self.playable_indices();
        if occupied.len() == 2 {
            let big = self.next_playable_after(dealer).unwrap_or(dealer);
            return (dealer, big);
        }

        let small = self.next_playable_after(dealer).unwrap_or(dealer);
        let big = self.next_playable_after(small).unwrap_or(small);
        (small, big)
    }

    fn seat_index(&self, user_id: Uuid) -> Option<usize> {
        self.seats.iter().position(|seat| *seat == Some(user_id))
    }

    fn remove_seat(&mut self, index: usize) {
        if let Some(user_id) = self.seats[index] {
            self.last_balances.insert(user_id, self.balances[index]);
        }
        self.seats[index] = None;
        self.balances[index] = 0;
        self.hole_cards[index].clear();
        self.folded[index] = false;
        self.all_in[index] = false;
        self.acted_this_street[index] = false;
        self.last_action[index] = None;
        self.leave_after_hand[index] = false;
        self.committed[index] = 0;
        self.street_bet[index] = 0;
        self.pending_commit[index] = None;

        if self.occupied_count() == 0 {
            self.dealer_button = None;
        }
    }

    fn record_activity(&mut self, user_id: Uuid) -> Option<u64> {
        let seat_index = self.seat_index(user_id)?;
        self.last_activity[seat_index] = Instant::now();
        self.activity_generation[seat_index] = self.activity_generation[seat_index].wrapping_add(1);
        Some(self.activity_generation[seat_index])
    }

    fn kick_inactive_user(
        &mut self,
        user_id: Uuid,
        activity_generation: u64,
    ) -> (bool, Vec<PokerSettlement>) {
        let Some(seat_index) = self.seat_index(user_id) else {
            return (false, Vec::new());
        };
        if self.activity_generation[seat_index] != activity_generation
            || self.last_activity[seat_index].elapsed()
                < Duration::from_secs(SEAT_IDLE_TIMEOUT_SECS)
        {
            return (false, Vec::new());
        }

        if self.phase.is_action_phase() && self.hole_cards[seat_index].len() == 2 {
            self.folded[seat_index] = true;
            self.acted_this_street[seat_index] = true;
            self.last_action[seat_index] = Some(PokerAction::Fold);
            self.leave_after_hand[seat_index] = true;
            let settlements = if self.active_seat == Some(seat_index) {
                self.advance_after_action(seat_index)
            } else if self.active_player_indices().len() == 1 {
                self.finish_by_fold(self.active_player_indices()[0])
            } else {
                Vec::new()
            };
            self.status_message = format!("Seat {} idle for 5m and folded.", seat_index + 1);
            return (true, settlements);
        }

        if !self.phase.is_action_phase()
            && self.phase != PokerPhase::PostingBlinds
            && !self.settlement_pending
        {
            self.remove_seat(seat_index);
            self.status_message = format!("Seat {} idle for 5m and left.", seat_index + 1);
            return (true, Vec::new());
        }

        (false, Vec::new())
    }
}

#[derive(Clone, Copy, Debug)]
struct CommitRequest {
    request_id: u64,
    user_id: Uuid,
    seat_index: usize,
    amount: i64,
}

#[derive(Clone, Copy, Debug)]
struct PendingCommit {
    request_id: u64,
    kind: CommitKind,
    amount: i64,
    target_bet: i64,
    raise_size: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CommitKind {
    SmallBlind,
    BigBlind,
    Call,
    BetRaise,
    AllIn,
}

enum ActionOutcome {
    None,
    Commit(CommitRequest),
    Settlements(Vec<PokerSettlement>),
}

#[derive(Clone, Debug)]
struct PokerSettlement {
    user_id: Uuid,
    credit: i64,
}

struct SidePot {
    amount: i64,
    eligible: Vec<usize>,
}

struct PotAward {
    amount: i64,
    winners: Vec<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct HandValue {
    category: u8,
    ranks: [u8; 5],
}

struct EvaluatedHand {
    value: HandValue,
    label: &'static str,
}

fn evaluate_best_hand(cards: &[PlayingCard]) -> EvaluatedHand {
    let ranks = rank_counts(cards);

    if let Some(high) = straight_flush_high(cards) {
        return evaluated(8, &[high], "straight flush");
    }

    let quads = ranks_with_count_at_least(&ranks, 4);
    if let Some(quad) = quads.first().copied() {
        let kicker = highest_excluding(&ranks, &[quad], 1);
        return evaluated(7, &[quad, kicker[0]], "four of a kind");
    }

    let trips = ranks_with_count_at_least(&ranks, 3);
    let pairs = ranks_with_count_at_least(&ranks, 2);
    if let Some(trip) = trips.first().copied()
        && let Some(pair) = pairs
            .iter()
            .copied()
            .find(|rank| *rank != trip)
            .or_else(|| trips.iter().copied().find(|rank| *rank != trip))
    {
        return evaluated(6, &[trip, pair], "full house");
    }

    if let Some(flush) = flush_ranks(cards) {
        return evaluated(5, &flush[..5], "flush");
    }

    if let Some(high) = straight_high(ranks.keys().copied().collect()) {
        return evaluated(4, &[high], "straight");
    }

    if let Some(trip) = trips.first().copied() {
        let kickers = highest_excluding(&ranks, &[trip], 2);
        return evaluated(3, &[trip, kickers[0], kickers[1]], "three of a kind");
    }

    if pairs.len() >= 2 {
        let high_pair = pairs[0];
        let low_pair = pairs[1];
        let kicker = highest_excluding(&ranks, &[high_pair, low_pair], 1);
        return evaluated(2, &[high_pair, low_pair, kicker[0]], "two pair");
    }

    if let Some(pair) = pairs.first().copied() {
        let kickers = highest_excluding(&ranks, &[pair], 3);
        return evaluated(1, &[pair, kickers[0], kickers[1], kickers[2]], "one pair");
    }

    let high_cards = highest_excluding(&ranks, &[], 5);
    evaluated(0, &high_cards, "high card")
}

fn evaluated(category: u8, ranks: &[u8], label: &'static str) -> EvaluatedHand {
    let mut normalized = [0; 5];
    for (index, rank) in ranks.iter().copied().take(5).enumerate() {
        normalized[index] = rank;
    }
    EvaluatedHand {
        value: HandValue {
            category,
            ranks: normalized,
        },
        label,
    }
}

fn rank_counts(cards: &[PlayingCard]) -> HashMap<u8, u8> {
    let mut counts = HashMap::new();
    for card in cards {
        *counts.entry(rank_value(card.rank)).or_insert(0) += 1;
    }
    counts
}

fn ranks_with_count_at_least(counts: &HashMap<u8, u8>, count: u8) -> Vec<u8> {
    let mut ranks = counts
        .iter()
        .filter_map(|(rank, rank_count)| (*rank_count >= count).then_some(*rank))
        .collect::<Vec<_>>();
    ranks.sort_unstable_by(|a, b| b.cmp(a));
    ranks
}

fn highest_excluding(counts: &HashMap<u8, u8>, excluded: &[u8], count: usize) -> Vec<u8> {
    let mut ranks = counts
        .keys()
        .copied()
        .filter(|rank| !excluded.contains(rank))
        .collect::<Vec<_>>();
    ranks.sort_unstable_by(|a, b| b.cmp(a));
    ranks.truncate(count);
    while ranks.len() < count {
        ranks.push(0);
    }
    ranks
}

fn flush_ranks(cards: &[PlayingCard]) -> Option<Vec<u8>> {
    for suit in [
        CardSuit::Hearts,
        CardSuit::Diamonds,
        CardSuit::Clubs,
        CardSuit::Spades,
    ] {
        let mut ranks = cards
            .iter()
            .filter_map(|card| (card.suit == suit).then_some(rank_value(card.rank)))
            .collect::<Vec<_>>();
        if ranks.len() < 5 {
            continue;
        }
        ranks.sort_unstable_by(|a, b| b.cmp(a));
        return Some(ranks);
    }
    None
}

fn straight_flush_high(cards: &[PlayingCard]) -> Option<u8> {
    let mut best = None;
    for suit in [
        CardSuit::Hearts,
        CardSuit::Diamonds,
        CardSuit::Clubs,
        CardSuit::Spades,
    ] {
        let ranks = cards
            .iter()
            .filter_map(|card| (card.suit == suit).then_some(rank_value(card.rank)))
            .collect::<Vec<_>>();
        if let Some(high) = straight_high(ranks) {
            best = best.max(Some(high));
        }
    }
    best
}

fn straight_high(mut ranks: Vec<u8>) -> Option<u8> {
    ranks.sort_unstable();
    ranks.dedup();
    if ranks.contains(&14) {
        ranks.insert(0, 1);
    }

    let mut run = 1;
    let mut best = None;
    for index in 1..ranks.len() {
        if ranks[index] == ranks[index - 1] + 1 {
            run += 1;
            if run >= 5 {
                best = Some(ranks[index]);
            }
        } else {
            run = 1;
        }
    }
    best
}

fn rank_value(rank: CardRank) -> u8 {
    match rank {
        CardRank::Ace => 14,
        CardRank::Number(value) => value,
        CardRank::Jack => 11,
        CardRank::Queen => 12,
        CardRank::King => 13,
    }
}

fn seat_list(seats: &[usize]) -> String {
    match seats {
        [] => "No seats".to_string(),
        [seat] => format!("Seat {}", seat + 1),
        _ => {
            let labels = seats
                .iter()
                .map(|seat| (seat + 1).to_string())
                .collect::<Vec<_>>()
                .join(", ");
            format!("Seats {labels}")
        }
    }
}

fn fresh_deck() -> Vec<PlayingCard> {
    let mut cards = Vec::with_capacity(52);
    for suit in [
        CardSuit::Hearts,
        CardSuit::Diamonds,
        CardSuit::Clubs,
        CardSuit::Spades,
    ] {
        cards.push(PlayingCard {
            suit,
            rank: CardRank::Ace,
        });
        for value in 2..=10 {
            cards.push(PlayingCard {
                suit,
                rank: CardRank::Number(value),
            });
        }
        cards.push(PlayingCard {
            suit,
            rank: CardRank::Jack,
        });
        cards.push(PlayingCard {
            suit,
            rank: CardRank::Queen,
        });
        cards.push(PlayingCard {
            suit,
            rank: CardRank::King,
        });
    }
    cards
}

fn shuffle(cards: &mut [PlayingCard]) {
    for idx in (1..cards.len()).rev() {
        let swap_idx = (OsRng.next_u64() as usize) % (idx + 1);
        cards.swap(idx, swap_idx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(rank: CardRank, suit: CardSuit) -> PlayingCard {
        PlayingCard { rank, suit }
    }

    fn uid(value: u128) -> Uuid {
        Uuid::from_u128(value)
    }

    fn seat_player(state: &mut SharedState, index: usize, user: Uuid, cards: Vec<PlayingCard>) {
        state.seats[index] = Some(user);
        state.balances[index] = 1_000;
        state.hole_cards[index] = cards;
    }

    fn credits_by_user(settlements: Vec<PokerSettlement>) -> HashMap<Uuid, i64> {
        settlements
            .into_iter()
            .map(|settlement| (settlement.user_id, settlement.credit))
            .collect()
    }

    #[test]
    fn ace_low_straight_is_scored_as_five_high() {
        let hand = evaluate_best_hand(&[
            c(CardRank::Ace, CardSuit::Spades),
            c(CardRank::Number(2), CardSuit::Hearts),
            c(CardRank::Number(3), CardSuit::Clubs),
            c(CardRank::Number(4), CardSuit::Diamonds),
            c(CardRank::Number(5), CardSuit::Spades),
            c(CardRank::King, CardSuit::Hearts),
            c(CardRank::Queen, CardSuit::Hearts),
        ]);

        assert_eq!(hand.value.category, 4);
        assert_eq!(hand.value.ranks[0], 5);
    }

    #[test]
    fn full_house_beats_flush() {
        let full_house = evaluate_best_hand(&[
            c(CardRank::Ace, CardSuit::Spades),
            c(CardRank::Ace, CardSuit::Hearts),
            c(CardRank::Ace, CardSuit::Clubs),
            c(CardRank::King, CardSuit::Diamonds),
            c(CardRank::King, CardSuit::Spades),
        ]);
        let flush = evaluate_best_hand(&[
            c(CardRank::Ace, CardSuit::Hearts),
            c(CardRank::Number(9), CardSuit::Hearts),
            c(CardRank::Number(7), CardSuit::Hearts),
            c(CardRank::Number(4), CardSuit::Hearts),
            c(CardRank::Number(2), CardSuit::Hearts),
        ]);

        assert!(full_house.value > flush.value);
    }

    #[test]
    fn side_pots_pay_short_all_in_and_side_winner() {
        let mut state = SharedState::new(uid(100));
        seat_player(
            &mut state,
            0,
            uid(1),
            vec![
                c(CardRank::Ace, CardSuit::Spades),
                c(CardRank::Ace, CardSuit::Hearts),
            ],
        );
        seat_player(
            &mut state,
            1,
            uid(2),
            vec![
                c(CardRank::King, CardSuit::Spades),
                c(CardRank::King, CardSuit::Hearts),
            ],
        );
        seat_player(
            &mut state,
            2,
            uid(3),
            vec![
                c(CardRank::Queen, CardSuit::Spades),
                c(CardRank::Queen, CardSuit::Hearts),
            ],
        );
        state.community = vec![
            c(CardRank::Number(2), CardSuit::Clubs),
            c(CardRank::Number(4), CardSuit::Diamonds),
            c(CardRank::Number(7), CardSuit::Clubs),
            c(CardRank::Number(9), CardSuit::Diamonds),
            c(CardRank::Jack, CardSuit::Clubs),
        ];
        state.committed = [50, 100, 100, 0];

        let settlements = state.finish_showdown();
        let credits = credits_by_user(settlements);

        assert_eq!(credits.get(&uid(1)), Some(&150));
        assert_eq!(credits.get(&uid(2)), Some(&100));
        assert_eq!(credits.get(&uid(3)), Some(&0));
    }

    #[test]
    fn short_all_in_raise_does_not_reopen_prior_actor_raises() {
        let mut state = SharedState::new(uid(100));
        seat_player(
            &mut state,
            0,
            uid(1),
            vec![
                c(CardRank::Ace, CardSuit::Spades),
                c(CardRank::Ace, CardSuit::Hearts),
            ],
        );
        seat_player(
            &mut state,
            1,
            uid(2),
            vec![
                c(CardRank::King, CardSuit::Spades),
                c(CardRank::King, CardSuit::Hearts),
            ],
        );
        seat_player(
            &mut state,
            2,
            uid(3),
            vec![
                c(CardRank::Queen, CardSuit::Spades),
                c(CardRank::Queen, CardSuit::Hearts),
            ],
        );
        state.phase = PokerPhase::PreFlop;
        state.active_seat = Some(2);
        state.current_bet = 100;
        state.min_raise = 100;
        state.committed = [100, 100, 0, 0];
        state.street_bet = [100, 100, 0, 0];
        state.acted_this_street = [true, true, false, false];
        state.balances[2] = 150;

        let short_all_in = match state.all_in(uid(3)) {
            ActionOutcome::Commit(request) => request,
            _ => panic!("short all-in should commit chips"),
        };
        assert_eq!(short_all_in.amount, 150);

        let settlements = state.apply_commit_success(short_all_in, 0);

        assert!(settlements.is_empty());
        assert_eq!(state.current_bet, 150);
        assert_eq!(state.min_raise, 100);
        assert_eq!(state.active_seat, Some(0));
        assert!(state.acted_this_street[0]);
        assert!(state.acted_this_street[1]);
        assert!(!state.can_raise(0));

        assert!(matches!(
            state.bet_or_raise(uid(1), 100),
            ActionOutcome::None
        ));
        let call = match state.call_or_check(uid(1)) {
            ActionOutcome::Commit(request) => request,
            _ => panic!("prior actor should still be able to call the extra chips"),
        };
        assert_eq!(call.amount, 50);
    }

    #[test]
    fn four_way_all_in_side_pots_pay_each_level() {
        let mut state = SharedState::new(uid(100));
        seat_player(
            &mut state,
            0,
            uid(1),
            vec![
                c(CardRank::Ace, CardSuit::Spades),
                c(CardRank::Ace, CardSuit::Hearts),
            ],
        );
        seat_player(
            &mut state,
            1,
            uid(2),
            vec![
                c(CardRank::King, CardSuit::Spades),
                c(CardRank::King, CardSuit::Hearts),
            ],
        );
        seat_player(
            &mut state,
            2,
            uid(3),
            vec![
                c(CardRank::Queen, CardSuit::Spades),
                c(CardRank::Queen, CardSuit::Hearts),
            ],
        );
        seat_player(
            &mut state,
            3,
            uid(4),
            vec![
                c(CardRank::Jack, CardSuit::Spades),
                c(CardRank::Jack, CardSuit::Hearts),
            ],
        );
        state.community = vec![
            c(CardRank::Number(2), CardSuit::Clubs),
            c(CardRank::Number(4), CardSuit::Diamonds),
            c(CardRank::Number(7), CardSuit::Clubs),
            c(CardRank::Number(9), CardSuit::Diamonds),
            c(CardRank::Number(10), CardSuit::Clubs),
        ];
        state.committed = [25, 50, 100, 200];

        let credits = credits_by_user(state.finish_showdown());

        assert_eq!(credits.get(&uid(1)), Some(&100));
        assert_eq!(credits.get(&uid(2)), Some(&75));
        assert_eq!(credits.get(&uid(3)), Some(&100));
        assert_eq!(credits.get(&uid(4)), Some(&100));
    }

    #[test]
    fn tied_side_pot_splits_only_among_eligible_players() {
        let mut state = SharedState::new(uid(100));
        seat_player(
            &mut state,
            0,
            uid(1),
            vec![
                c(CardRank::Ace, CardSuit::Spades),
                c(CardRank::King, CardSuit::Hearts),
            ],
        );
        seat_player(
            &mut state,
            1,
            uid(2),
            vec![
                c(CardRank::Queen, CardSuit::Spades),
                c(CardRank::Jack, CardSuit::Hearts),
            ],
        );
        seat_player(
            &mut state,
            2,
            uid(3),
            vec![
                c(CardRank::Number(10), CardSuit::Spades),
                c(CardRank::Number(9), CardSuit::Hearts),
            ],
        );
        state.community = vec![
            c(CardRank::Number(2), CardSuit::Clubs),
            c(CardRank::Number(3), CardSuit::Diamonds),
            c(CardRank::Number(4), CardSuit::Clubs),
            c(CardRank::Number(5), CardSuit::Diamonds),
            c(CardRank::Number(6), CardSuit::Clubs),
        ];
        state.committed = [50, 100, 100, 0];

        let credits = credits_by_user(state.finish_showdown());

        assert_eq!(credits.get(&uid(1)), Some(&50));
        assert_eq!(credits.get(&uid(2)), Some(&100));
        assert_eq!(credits.get(&uid(3)), Some(&100));
    }

    #[test]
    fn fold_win_does_not_reveal_winner_cards() {
        let mut state = SharedState::new(uid(100));
        seat_player(
            &mut state,
            0,
            uid(1),
            vec![
                c(CardRank::Ace, CardSuit::Spades),
                c(CardRank::Ace, CardSuit::Hearts),
            ],
        );
        seat_player(
            &mut state,
            1,
            uid(2),
            vec![
                c(CardRank::King, CardSuit::Spades),
                c(CardRank::King, CardSuit::Hearts),
            ],
        );
        state.committed = [20, 20, 0, 0];
        state.folded[1] = true;

        let settlements = state.finish_by_fold(0);

        assert_eq!(settlements.len(), 2);
        assert!(state.seat_snapshot(0).revealed_cards.is_none());
        assert_eq!(state.winners, vec![0]);
    }
}
