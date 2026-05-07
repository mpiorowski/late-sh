use std::{
    collections::HashMap,
    sync::{Arc, Mutex as StdMutex},
    time::{Duration, Instant},
};

use late_core::MutexRecover;
use rand_core::{OsRng, RngCore};
use tokio::sync::{Mutex, watch};
use uuid::Uuid;

use crate::app::games::cards::{CardRank, CardSuit, PlayingCard};

pub const MAX_SEATS: usize = 4;

const SEAT_IDLE_TIMEOUT_SECS: u64 = 5 * 60;

#[derive(Clone)]
pub struct PokerService {
    room_id: Uuid,
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
}

#[derive(Clone, Debug, Default)]
pub struct PokerPrivateSnapshot {
    pub hole_cards: Vec<PlayingCard>,
    pub notice: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PokerPhase {
    Waiting,
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
    Check,
    Fold,
}

impl PokerAction {
    pub fn label(self) -> &'static str {
        match self {
            Self::Check => "Check",
            Self::Fold => "Fold",
        }
    }
}

impl PokerService {
    pub fn new(room_id: Uuid) -> Self {
        let state = SharedState::new(room_id);
        let initial_snapshot = state.public_snapshot();
        let (public_tx, public_rx) = watch::channel(initial_snapshot);
        Self {
            room_id,
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

    pub fn sit_task(&self, user_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            let activity_generation = {
                let mut state = svc.state.lock().await;
                state.sit(user_id);
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
            let mut state = svc.state.lock().await;
            state.leave(user_id);
            svc.publish(&state);
        });
    }

    pub fn start_hand_task(&self, user_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            let activity_generation = {
                let mut state = svc.state.lock().await;
                state.start_hand(user_id);
                let activity_generation = state.record_activity(user_id);
                svc.publish(&state);
                activity_generation
            };
            if let Some(activity_generation) = activity_generation {
                svc.schedule_inactivity_kick(user_id, activity_generation);
            }
        });
    }

    pub fn check_task(&self, user_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            let activity_generation = {
                let mut state = svc.state.lock().await;
                state.player_action(user_id, PokerAction::Check);
                let activity_generation = state.record_activity(user_id);
                svc.publish(&state);
                activity_generation
            };
            if let Some(activity_generation) = activity_generation {
                svc.schedule_inactivity_kick(user_id, activity_generation);
            }
        });
    }

    pub fn fold_task(&self, user_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            let activity_generation = {
                let mut state = svc.state.lock().await;
                state.player_action(user_id, PokerAction::Fold);
                let activity_generation = state.record_activity(user_id);
                svc.publish(&state);
                activity_generation
            };
            if let Some(activity_generation) = activity_generation {
                svc.schedule_inactivity_kick(user_id, activity_generation);
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

    fn schedule_inactivity_kick(&self, user_id: Uuid, activity_generation: u64) {
        let svc = self.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(SEAT_IDLE_TIMEOUT_SECS)).await;

            let mut state = svc.state.lock().await;
            if state.kick_inactive_user(user_id, activity_generation) {
                svc.publish(&state);
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
    hole_cards: [Vec<PlayingCard>; MAX_SEATS],
    folded: [bool; MAX_SEATS],
    acted_this_street: [bool; MAX_SEATS],
    last_action: [Option<PokerAction>; MAX_SEATS],
    community: Vec<PlayingCard>,
    deck: Vec<PlayingCard>,
    dealer_button: Option<usize>,
    active_seat: Option<usize>,
    phase: PokerPhase,
    hand_number: u64,
    winners: Vec<usize>,
    winning_rank: Option<String>,
    status_message: String,
    last_activity: [Instant; MAX_SEATS],
    activity_generation: [u64; MAX_SEATS],
}

impl SharedState {
    fn new(room_id: Uuid) -> Self {
        let now = Instant::now();
        Self {
            room_id,
            seats: [None; MAX_SEATS],
            hole_cards: std::array::from_fn(|_| Vec::new()),
            folded: [false; MAX_SEATS],
            acted_this_street: [false; MAX_SEATS],
            last_action: [None; MAX_SEATS],
            community: Vec::new(),
            deck: Vec::new(),
            dealer_button: None,
            active_seat: None,
            phase: PokerPhase::Waiting,
            hand_number: 0,
            winners: Vec::new(),
            winning_rank: None,
            status_message: "Take a seat. Two players can deal a hand.".to_string(),
            last_activity: [now; MAX_SEATS],
            activity_generation: [0; MAX_SEATS],
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
        }
    }

    fn private_snapshot_for(&self, user_id: Uuid) -> PokerPrivateSnapshot {
        let Some(index) = self.seat_index(user_id) else {
            return PokerPrivateSnapshot::default();
        };
        let hole_cards = self.hole_cards[index].clone();
        let notice = if hole_cards.is_empty() {
            None
        } else {
            Some("Your hole cards are private.".to_string())
        };
        PokerPrivateSnapshot { hole_cards, notice }
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
        }
    }

    fn revealed_cards_for(&self, index: usize) -> Option<Vec<PlayingCard>> {
        if self.phase != PokerPhase::Showdown
            || self.seats[index].is_none()
            || self.folded[index]
            || self.hole_cards[index].len() != 2
        {
            return None;
        }
        Some(self.hole_cards[index].clone())
    }

    fn sit(&mut self, user_id: Uuid) {
        if self.seat_index(user_id).is_some() {
            return;
        }
        let Some(index) = self.seats.iter().position(Option::is_none) else {
            self.status_message = "Poker table is full.".to_string();
            return;
        };
        self.seats[index] = Some(user_id);
        self.status_message = if self.occupied_count() >= 2 {
            "Press n to deal a hand.".to_string()
        } else {
            "Waiting for a second player.".to_string()
        };
    }

    fn leave(&mut self, user_id: Uuid) {
        let Some(index) = self.seat_index(user_id) else {
            return;
        };
        self.remove_seat(index);
        if self.phase == PokerPhase::Waiting {
            self.status_message = if self.occupied_count() == 0 {
                "Take a seat. Two players can deal a hand.".to_string()
            } else {
                "Waiting for players.".to_string()
            };
        } else if self.phase != PokerPhase::Showdown {
            self.status_message = format!("Seat {} left the hand.", index + 1);
        }
    }

    fn start_hand(&mut self, user_id: Uuid) {
        if self.seat_index(user_id).is_none() {
            self.status_message = "Sit before dealing a hand.".to_string();
            return;
        }
        if !matches!(self.phase, PokerPhase::Waiting | PokerPhase::Showdown) {
            self.status_message = "Finish the current hand first.".to_string();
            return;
        }
        if self.occupied_count() < 2 {
            self.status_message = "Need at least two players to deal.".to_string();
            return;
        }

        self.deck = fresh_deck();
        shuffle(&mut self.deck);
        self.community.clear();
        self.hole_cards = std::array::from_fn(|_| Vec::new());
        self.folded = [false; MAX_SEATS];
        self.acted_this_street = [false; MAX_SEATS];
        self.last_action = [None; MAX_SEATS];
        self.winners.clear();
        self.winning_rank = None;

        let dealer = self
            .dealer_button
            .and_then(|index| self.next_occupied_after(index))
            .or_else(|| self.seat_index(user_id))
            .or_else(|| self.occupied_indices().into_iter().next())
            .unwrap_or(0);
        self.dealer_button = Some(dealer);

        let occupied = self.occupied_indices();
        for _ in 0..2 {
            for index in &occupied {
                if let Some(card) = self.deck.pop() {
                    self.hole_cards[*index].push(card);
                }
            }
        }

        self.phase = PokerPhase::PreFlop;
        self.hand_number = self.hand_number.saturating_add(1);
        self.active_seat = self.next_active_after(dealer);
        self.status_message = match self.active_seat {
            Some(index) => format!("Hand {} dealt. Seat {} acts.", self.hand_number, index + 1),
            None => "Hand dealt.".to_string(),
        };
    }

    fn player_action(&mut self, user_id: Uuid, action: PokerAction) {
        if !self.phase.is_action_phase() {
            self.status_message = "No poker action is pending.".to_string();
            return;
        }
        let Some(index) = self.seat_index(user_id) else {
            self.status_message = "Sit before playing.".to_string();
            return;
        };
        if self.active_seat != Some(index) {
            self.status_message = match self.active_seat {
                Some(active) => format!("Seat {} acts now.", active + 1),
                None => "No active player.".to_string(),
            };
            return;
        }
        if self.hole_cards[index].len() != 2 || self.folded[index] {
            self.status_message = "You are not in this hand.".to_string();
            return;
        }

        match action {
            PokerAction::Check => {
                self.acted_this_street[index] = true;
                self.last_action[index] = Some(PokerAction::Check);
            }
            PokerAction::Fold => {
                self.folded[index] = true;
                self.acted_this_street[index] = true;
                self.last_action[index] = Some(PokerAction::Fold);
            }
        }

        self.advance_after_action(index);
    }

    fn advance_after_action(&mut self, acted_index: usize) {
        let active_players = self.active_player_indices();
        if active_players.len() == 1 {
            self.finish_by_fold(active_players[0]);
            return;
        }
        if active_players.is_empty() {
            self.phase = PokerPhase::Waiting;
            self.active_seat = None;
            self.status_message = "Hand ended. Waiting for players.".to_string();
            return;
        }

        if self.street_complete() {
            self.advance_street();
            return;
        }

        self.active_seat = self.next_active_after(acted_index);
        self.status_message = match self.active_seat {
            Some(index) => format!("Seat {} acts.", index + 1),
            None => "Waiting for action.".to_string(),
        };
    }

    fn advance_street(&mut self) {
        self.acted_this_street = [false; MAX_SEATS];
        match self.phase {
            PokerPhase::PreFlop => {
                self.deal_community(3);
                self.phase = PokerPhase::Flop;
                self.set_first_action_for_new_street("Flop dealt");
            }
            PokerPhase::Flop => {
                self.deal_community(1);
                self.phase = PokerPhase::Turn;
                self.set_first_action_for_new_street("Turn dealt");
            }
            PokerPhase::Turn => {
                self.deal_community(1);
                self.phase = PokerPhase::River;
                self.set_first_action_for_new_street("River dealt");
            }
            PokerPhase::River => self.finish_showdown(),
            _ => {}
        }
    }

    fn set_first_action_for_new_street(&mut self, prefix: &'static str) {
        let dealer = self.dealer_button.unwrap_or(0);
        self.active_seat = self.next_active_after(dealer);
        self.status_message = match self.active_seat {
            Some(index) => format!("{prefix}. Seat {} acts.", index + 1),
            None => format!("{prefix}."),
        };
    }

    fn finish_by_fold(&mut self, winner: usize) {
        self.phase = PokerPhase::Showdown;
        self.active_seat = None;
        self.winners = vec![winner];
        self.winning_rank = None;
        self.status_message = format!("Seat {} wins by fold. Press n for next hand.", winner + 1);
    }

    fn finish_showdown(&mut self) {
        let contenders = self.active_player_indices();
        if contenders.is_empty() {
            self.phase = PokerPhase::Waiting;
            self.active_seat = None;
            self.status_message = "No contenders remain.".to_string();
            return;
        }

        let mut scored = Vec::with_capacity(contenders.len());
        for index in contenders {
            let mut cards = self.hole_cards[index].clone();
            cards.extend(self.community.iter().copied());
            scored.push((index, evaluate_best_hand(&cards)));
        }
        let Some(best) = scored.iter().map(|(_, hand)| hand.value).max() else {
            return;
        };
        self.winners = scored
            .iter()
            .filter_map(|(index, hand)| (hand.value == best).then_some(*index))
            .collect();
        self.winning_rank = scored
            .iter()
            .find_map(|(_, hand)| (hand.value == best).then_some(hand.label.to_string()));
        self.phase = PokerPhase::Showdown;
        self.active_seat = None;

        let winners = seat_list(&self.winners);
        let rank = self
            .winning_rank
            .as_deref()
            .unwrap_or("best hand")
            .to_string();
        self.status_message = format!("{winners} win with {rank}. Press n for next hand.");
    }

    fn remove_seat(&mut self, index: usize) {
        self.seats[index] = None;
        self.hole_cards[index].clear();
        self.folded[index] = false;
        self.acted_this_street[index] = false;
        self.last_action[index] = None;

        if self.occupied_count() == 0 {
            self.dealer_button = None;
        }

        if !self.phase.is_action_phase() {
            return;
        }

        let active_players = self.active_player_indices();
        match active_players.len() {
            0 => {
                self.phase = PokerPhase::Waiting;
                self.active_seat = None;
            }
            1 => self.finish_by_fold(active_players[0]),
            _ if self.active_seat == Some(index) => {
                if self.street_complete() {
                    self.advance_street();
                } else {
                    self.active_seat = self.next_active_after(index);
                }
            }
            _ => {}
        }
    }

    fn deal_community(&mut self, count: usize) {
        for _ in 0..count {
            if let Some(card) = self.deck.pop() {
                self.community.push(card);
            }
        }
    }

    fn street_complete(&self) -> bool {
        self.active_player_indices()
            .into_iter()
            .all(|index| self.acted_this_street[index])
    }

    fn occupied_count(&self) -> usize {
        self.seats.iter().filter(|seat| seat.is_some()).count()
    }

    fn occupied_indices(&self) -> Vec<usize> {
        self.seats
            .iter()
            .enumerate()
            .filter_map(|(index, seat)| seat.is_some().then_some(index))
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

    fn next_occupied_after(&self, start: usize) -> Option<usize> {
        (1..=MAX_SEATS)
            .map(|offset| (start + offset) % MAX_SEATS)
            .find(|index| self.seats[*index].is_some())
    }

    fn next_active_after(&self, start: usize) -> Option<usize> {
        (1..=MAX_SEATS)
            .map(|offset| (start + offset) % MAX_SEATS)
            .find(|index| {
                self.seats[*index].is_some()
                    && self.hole_cards[*index].len() == 2
                    && !self.folded[*index]
            })
    }

    fn seat_index(&self, user_id: Uuid) -> Option<usize> {
        self.seats.iter().position(|seat| *seat == Some(user_id))
    }

    fn record_activity(&mut self, user_id: Uuid) -> Option<u64> {
        let seat_index = self.seat_index(user_id)?;
        self.last_activity[seat_index] = Instant::now();
        self.activity_generation[seat_index] = self.activity_generation[seat_index].wrapping_add(1);
        Some(self.activity_generation[seat_index])
    }

    fn kick_inactive_user(&mut self, user_id: Uuid, activity_generation: u64) -> bool {
        let Some(seat_index) = self.seat_index(user_id) else {
            return false;
        };
        if self.activity_generation[seat_index] != activity_generation
            || self.last_activity[seat_index].elapsed()
                < Duration::from_secs(SEAT_IDLE_TIMEOUT_SECS)
        {
            return false;
        }

        self.remove_seat(seat_index);
        if self.phase == PokerPhase::Waiting {
            self.status_message = format!("Seat {} idle for 5m and left.", seat_index + 1);
        } else if self.phase != PokerPhase::Showdown {
            self.status_message = format!("Seat {} idle for 5m and folded.", seat_index + 1);
        }
        true
    }
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
}
