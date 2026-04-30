use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use tokio::sync::{
    broadcast::{self, error::TryRecvError},
    watch,
};
use uuid::Uuid;

use crate::app::{
    games::cards::{CardRank, CardSuit, PlayingCard},
    rooms::blackjack::{
        player::BlackjackPlayerInfo,
        svc::{BlackjackEvent, BlackjackService},
    },
};

pub const MIN_BET: i64 = 10;
pub const MAX_BET: i64 = 100;
pub const MAX_SEATS: usize = 4;
pub const BLACKJACK_TARGET: u8 = 21;
pub const DEALER_STAND_ON: u8 = 17;
pub const SHOE_DECKS: usize = 6;
pub const SHOE_PENETRATION: usize = 52;

pub const DEALER_STANDS_ON_SOFT_17: bool = true;

pub fn card_value(card: PlayingCard) -> u8 {
    match card.rank {
        CardRank::Ace => 1,
        CardRank::Number(n) => n,
        CardRank::Jack | CardRank::Queen | CardRank::King => 10,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HandScore {
    pub total: u8,
    pub soft: bool,
}

pub fn score(cards: &[PlayingCard]) -> HandScore {
    let mut total: u8 = 0;
    let mut aces: u8 = 0;
    for c in cards {
        total += card_value(*c);
        if matches!(c.rank, CardRank::Ace) {
            aces += 1;
        }
    }
    let mut soft = false;
    while aces > 0 && total + 10 <= BLACKJACK_TARGET {
        total += 10;
        aces -= 1;
        soft = true;
    }
    HandScore { total, soft }
}

pub fn is_bust(cards: &[PlayingCard]) -> bool {
    score(cards).total > BLACKJACK_TARGET
}

pub fn is_natural_blackjack(cards: &[PlayingCard]) -> bool {
    cards.len() == 2 && score(cards).total == BLACKJACK_TARGET
}

pub fn can_double(cards: &[PlayingCard]) -> bool {
    cards.len() == 2
}

pub fn can_split(cards: &[PlayingCard]) -> bool {
    cards.len() == 2 && card_value(cards[0]) == card_value(cards[1])
}

pub fn dealer_must_hit(cards: &[PlayingCard]) -> bool {
    let s = score(cards);
    if s.total < DEALER_STAND_ON {
        return true;
    }
    if s.total == DEALER_STAND_ON && s.soft && !DEALER_STANDS_ON_SOFT_17 {
        return true;
    }
    false
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bet(i64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BetError {
    BelowMin,
    AboveMax,
}

impl Bet {
    pub fn new(amount: i64) -> Result<Self, BetError> {
        Self::new_for_table(amount, MIN_BET, MAX_BET)
    }

    pub fn new_for_table(amount: i64, min_bet: i64, max_bet: i64) -> Result<Self, BetError> {
        if amount < min_bet {
            return Err(BetError::BelowMin);
        }
        if amount > max_bet {
            return Err(BetError::AboveMax);
        }
        Ok(Self(amount))
    }

    pub fn amount(self) -> i64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Outcome {
    PlayerBlackjack,
    PlayerWin,
    Push,
    DealerWin,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeatAction {
    Sit,
    Bet,
    Hit,
    Stand,
    MissedDeal,
}

pub fn settle(player: &[PlayingCard], dealer: &[PlayingCard]) -> Outcome {
    if is_bust(player) {
        return Outcome::DealerWin;
    }
    let player_bj = is_natural_blackjack(player);
    let dealer_bj = is_natural_blackjack(dealer);
    match (player_bj, dealer_bj) {
        (true, true) => return Outcome::Push,
        (true, false) => return Outcome::PlayerBlackjack,
        _ => {}
    }
    if is_bust(dealer) {
        return Outcome::PlayerWin;
    }
    let p = score(player).total;
    let d = score(dealer).total;
    match p.cmp(&d) {
        std::cmp::Ordering::Greater => Outcome::PlayerWin,
        std::cmp::Ordering::Less => Outcome::DealerWin,
        std::cmp::Ordering::Equal => Outcome::Push,
    }
}

pub fn payout_credit(bet: Bet, outcome: Outcome) -> i64 {
    let b = bet.amount();
    match outcome {
        Outcome::DealerWin => 0,
        Outcome::Push => b,
        Outcome::PlayerWin => b * 2,
        Outcome::PlayerBlackjack => b * 2 + b / 2,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Betting,
    BetPending,
    PlayerTurn,
    DealerTurn,
    Settling,
}

impl Phase {
    pub fn label(self) -> &'static str {
        match self {
            Self::Betting => "Betting",
            Self::BetPending => "BetPending",
            Self::PlayerTurn => "PlayerTurn",
            Self::DealerTurn => "DealerTurn",
            Self::Settling => "Settling",
        }
    }
}

#[derive(Clone, Debug)]
pub struct BlackjackSnapshot {
    pub balance: i64,
    pub seats: Vec<BlackjackSeat>,
    pub betting_countdown_secs: Option<u64>,
    pub action_countdown_secs: Option<u64>,
    pub dealer_hand: Vec<PlayingCard>,
    /// User-facing active/reference hand. The authoritative hands live on seats.
    pub player_hand: Vec<PlayingCard>,
    pub current_bet_amount: Option<i64>,
    pub min_bet: i64,
    pub max_bet: i64,
    pub chip_denominations: Vec<i64>,
    pub phase: Phase,
    pub last_outcome: Option<Outcome>,
    pub last_net_change: i64,
    pub stake_chips: Vec<i64>,
    pub selected_chip_index: usize,
    pub status_message: String,
    pub private_notice: Option<String>,
    pub dealer_revealed: bool,
    pub dealer_score: Option<HandScore>,
    pub player_score: Option<HandScore>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlackjackSeat {
    pub index: usize,
    pub user_id: Option<Uuid>,
    pub player: Option<BlackjackPlayerInfo>,
    pub bet_amount: Option<i64>,
    pub stake_chips: Vec<i64>,
    pub hand: Vec<PlayingCard>,
    pub phase: SeatPhase,
    pub score: Option<HandScore>,
    pub last_outcome: Option<Outcome>,
    pub last_action: Option<SeatAction>,
    pub last_net_change: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeatPhase {
    Empty,
    Seated,
    BetPending,
    Ready,
    Playing,
    Stood,
    Settled,
}

impl SeatPhase {
    pub fn label(self) -> &'static str {
        match self {
            Self::Empty => "Open",
            Self::Seated => "Seated",
            Self::BetPending => "BetPending",
            Self::Ready => "Ready",
            Self::Playing => "Playing",
            Self::Stood => "Stood",
            Self::Settled => "Settled",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Shoe {
    cards: Vec<PlayingCard>,
    penetration: usize,
}

impl Shoe {
    pub fn new() -> Self {
        let mut shoe = Self {
            cards: fresh_shoe(),
            penetration: SHOE_PENETRATION,
        };
        shuffle(&mut shoe.cards);
        shoe
    }

    pub fn draw(&mut self) -> PlayingCard {
        if self.cards.len() <= self.penetration {
            self.cards = fresh_shoe();
            shuffle(&mut self.cards);
        }
        self.cards.pop().expect("shoe should never be empty")
    }

    #[cfg(test)]
    fn from_top(top_cards: Vec<PlayingCard>) -> Self {
        let mut cards = top_cards;
        cards.reverse();
        Self {
            cards,
            penetration: 0,
        }
    }
}

impl Default for Shoe {
    fn default() -> Self {
        Self::new()
    }
}

pub struct State {
    user_id: Uuid,
    pub(crate) balance: i64,
    pub(crate) selected_chip_index: usize,
    pub(crate) snapshot: BlackjackSnapshot,
    pub(crate) private_notice: Option<String>,
    pending_request_id: Option<Uuid>,
    svc: BlackjackService,
    snapshot_rx: watch::Receiver<BlackjackSnapshot>,
    event_rx: broadcast::Receiver<BlackjackEvent>,
}

impl State {
    pub fn new(svc: BlackjackService, user_id: Uuid, balance: i64) -> Self {
        let snapshot_rx = svc.subscribe_state();
        let snapshot = snapshot_rx.borrow().clone();
        let event_rx = svc.subscribe_events();
        Self {
            user_id,
            balance,
            selected_chip_index: 0,
            snapshot,
            private_notice: None,
            pending_request_id: None,
            svc,
            snapshot_rx,
            event_rx,
        }
    }

    pub fn tick(&mut self) {
        if self.snapshot_rx.has_changed().unwrap_or(false) {
            self.snapshot = self.snapshot_rx.borrow_and_update().clone();
        }

        loop {
            match self.event_rx.try_recv() {
                Ok(event) => self.apply_event(event),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Closed) => break,
                Err(TryRecvError::Lagged(skipped)) => {
                    self.private_notice =
                        Some(format!("Blackjack updates lagged ({skipped} dropped)."));
                }
            }
        }
    }

    pub fn move_chip_selection(&mut self, delta: isize) {
        let len = self.snapshot.chip_denominations.len() as isize;
        if len == 0 {
            self.selected_chip_index = 0;
            return;
        }
        let current = self.selected_chip_index as isize;
        self.selected_chip_index = (current + delta).rem_euclid(len) as usize;
    }

    pub fn selected_chip_value(&self) -> i64 {
        self.snapshot
            .chip_denominations
            .get(self.selected_chip_index)
            .copied()
            .unwrap_or(self.snapshot.min_bet)
    }

    pub fn throw_selected_chip(&mut self) {
        if self.snapshot.phase != Phase::Betting || !self.is_seated() {
            return;
        }
        if self.user_seat().and_then(|seat| seat.bet_amount).is_some() {
            self.private_notice = Some("Bet already placed. Dealer will deal soon.".to_string());
            return;
        }
        let chip = self.selected_chip_value();
        let next_amount = self.stake_amount() + chip;
        if next_amount > self.snapshot.max_bet {
            self.private_notice = Some(format!("Table max is {} chips.", self.snapshot.max_bet));
            return;
        }
        if next_amount > self.balance {
            self.private_notice = Some(format!("Only {} chips available to bet.", self.balance));
            return;
        }
        self.private_notice = None;
        self.svc.throw_chip_task(self.user_id, chip);
    }

    pub fn pull_last_chip(&mut self) {
        if self.snapshot.phase == Phase::Betting {
            self.private_notice = None;
            self.svc.pull_stake_chip_task(self.user_id);
        }
    }

    pub fn clear_stake(&mut self) {
        if self.snapshot.phase == Phase::Betting {
            self.private_notice = None;
            self.svc.clear_stake_task(self.user_id);
        }
    }

    pub fn submit_stake(&mut self) {
        if self.snapshot.phase != Phase::Betting {
            return;
        }
        if !self.is_seated() {
            self.private_notice = Some("Sit before betting.".to_string());
            return;
        }
        if self.user_seat().and_then(|seat| seat.bet_amount).is_some() {
            self.private_notice = Some("Bet already placed. Dealer will deal soon.".to_string());
            return;
        }
        let amount = self.stake_amount();
        if amount == 0 {
            self.private_notice = Some("Throw chips onto the stake first.".to_string());
            return;
        }
        if amount > self.balance {
            self.private_notice = Some(format!("Only {} chips available to bet.", self.balance));
            return;
        }
        let request_id = Uuid::now_v7();
        self.pending_request_id = Some(request_id);
        self.private_notice = None;
        self.svc.submit_stake_task(self.user_id, request_id);
    }

    pub fn sit(&mut self) {
        self.private_notice = None;
        self.svc.sit_task(self.user_id);
    }

    pub fn leave_seat(&mut self) {
        self.private_notice = None;
        self.svc.leave_seat_task(self.user_id);
    }

    pub fn hit(&mut self) {
        self.svc.hit_task(self.user_id);
    }

    pub fn stand(&mut self) {
        self.svc.stand_task(self.user_id);
    }

    pub fn next_hand(&mut self) {
        self.svc.next_hand_task(self.user_id);
    }

    pub fn current_bet_amount(&self) -> Option<i64> {
        self.user_seat()
            .and_then(|seat| seat.bet_amount)
            .or(self.snapshot.current_bet_amount)
    }

    pub fn stake_amount(&self) -> i64 {
        self.user_seat()
            .map(|seat| seat.stake_chips.iter().sum())
            .unwrap_or(0)
    }

    pub fn seat_index(&self) -> Option<usize> {
        self.snapshot
            .seats
            .iter()
            .find(|seat| seat.user_id == Some(self.user_id))
            .map(|seat| seat.index)
    }

    pub fn is_seated(&self) -> bool {
        self.seat_index().is_some()
    }

    pub fn can_act(&self) -> bool {
        self.user_seat()
            .is_some_and(|seat| seat.phase == SeatPhase::Playing)
    }

    pub fn user_seat(&self) -> Option<&BlackjackSeat> {
        self.snapshot
            .seats
            .iter()
            .find(|seat| seat.user_id == Some(self.user_id))
    }

    fn active_seat(&self) -> Option<&BlackjackSeat> {
        self.snapshot
            .seats
            .iter()
            .find(|seat| seat.phase == SeatPhase::Playing)
    }

    pub fn snapshot(&self) -> BlackjackSnapshot {
        let mut snapshot = self.snapshot.clone();
        let reference_seat = if self.can_act() {
            self.user_seat()
        } else {
            self.active_seat().or_else(|| self.user_seat())
        };
        snapshot.balance = self.balance;
        snapshot.stake_chips = self
            .user_seat()
            .map(|seat| seat.stake_chips.clone())
            .unwrap_or_default();
        snapshot.selected_chip_index = self
            .selected_chip_index
            .min(snapshot.chip_denominations.len().saturating_sub(1));
        snapshot.current_bet_amount = self.current_bet_amount();
        if let Some(seat) = reference_seat {
            snapshot.player_hand = seat.hand.clone();
            snapshot.player_score = seat.score;
        }
        if let Some(user_seat) = self.user_seat() {
            snapshot.last_outcome = user_seat.last_outcome;
            snapshot.last_net_change = user_seat.last_net_change;
        }
        snapshot.private_notice = self.user_facing_notice();
        snapshot
    }

    fn user_facing_notice(&self) -> Option<String> {
        if let Some(notice) = &self.private_notice {
            return Some(notice.clone());
        }
        if self.snapshot.phase == Phase::Betting
            && self.is_seated()
            && self.user_seat().and_then(|seat| seat.bet_amount).is_none()
            && self.balance < self.snapshot.min_bet
        {
            return Some("You need more chips to bet.".to_string());
        }
        None
    }

    pub fn player_score(&self) -> Option<HandScore> {
        self.snapshot.player_score
    }

    pub fn dealer_score(&self) -> Option<HandScore> {
        self.snapshot.dealer_score
    }

    pub fn dealer_revealed(&self) -> bool {
        self.snapshot.dealer_revealed
    }

    fn apply_event(&mut self, event: BlackjackEvent) {
        match event {
            BlackjackEvent::BetPlaced {
                user_id,
                request_id,
                result,
            } => {
                if user_id != self.user_id || Some(request_id) != self.pending_request_id {
                    return;
                }
                self.pending_request_id = None;
                match result {
                    Ok(new_balance) => {
                        self.balance = new_balance;
                        self.private_notice = None;
                    }
                    Err(message) => {
                        self.private_notice = Some(message);
                    }
                }
            }
            BlackjackEvent::SeatJoined { user_id, .. } => {
                if user_id == self.user_id {
                    self.private_notice = None;
                }
            }
            BlackjackEvent::SeatLeft { user_id, .. } => {
                if user_id == self.user_id {
                    self.private_notice = None;
                }
            }
            BlackjackEvent::HandSettled {
                user_id,
                new_balance,
                ..
            } => {
                if user_id == self.user_id {
                    self.balance = new_balance;
                }
            }
            BlackjackEvent::ActionError { user_id, message } => {
                if user_id == self.user_id {
                    self.private_notice = Some(message);
                }
            }
        }
    }
}

fn fresh_shoe() -> Vec<PlayingCard> {
    let mut cards = Vec::with_capacity(SHOE_DECKS * 52);
    for _ in 0..SHOE_DECKS {
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
            for n in 2..=10 {
                cards.push(PlayingCard {
                    suit,
                    rank: CardRank::Number(n),
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

    fn ace() -> PlayingCard {
        c(CardRank::Ace, CardSuit::Spades)
    }
    fn king() -> PlayingCard {
        c(CardRank::King, CardSuit::Hearts)
    }
    fn queen() -> PlayingCard {
        c(CardRank::Queen, CardSuit::Diamonds)
    }
    fn ten() -> PlayingCard {
        c(CardRank::Number(10), CardSuit::Clubs)
    }
    fn nine() -> PlayingCard {
        c(CardRank::Number(9), CardSuit::Clubs)
    }
    fn seven() -> PlayingCard {
        c(CardRank::Number(7), CardSuit::Spades)
    }
    fn five() -> PlayingCard {
        c(CardRank::Number(5), CardSuit::Hearts)
    }

    #[test]
    fn ace_plus_king_is_soft_21() {
        let s = score(&[ace(), king()]);
        assert_eq!(
            s,
            HandScore {
                total: 21,
                soft: true
            }
        );
    }

    #[test]
    fn pair_of_aces_is_soft_12() {
        let s = score(&[ace(), ace()]);
        assert_eq!(
            s,
            HandScore {
                total: 12,
                soft: true
            }
        );
    }

    #[test]
    fn triple_ace_plus_nine_is_soft_21() {
        let s = score(&[ace(), ace(), nine()]);
        assert_eq!(
            s,
            HandScore {
                total: 21,
                soft: true
            }
        );
    }

    #[test]
    fn ace_plus_ace_plus_king_is_hard_12() {
        let s = score(&[ace(), ace(), king()]);
        assert_eq!(
            s,
            HandScore {
                total: 12,
                soft: false
            }
        );
    }

    #[test]
    fn three_face_cards_is_hard_bust() {
        let s = score(&[king(), queen(), ten()]);
        assert_eq!(s.total, 30);
        assert!(!s.soft);
        assert!(is_bust(&[king(), queen(), ten()]));
    }

    #[test]
    fn natural_blackjack_requires_exactly_two_cards() {
        assert!(is_natural_blackjack(&[ace(), king()]));
        assert!(!is_natural_blackjack(&[five(), five(), ace()]));
    }

    #[test]
    fn can_split_uses_point_value_not_rank() {
        assert!(can_split(&[king(), queen()]));
        assert!(can_split(&[ace(), ace()]));
        assert!(!can_split(&[king(), nine()]));
        assert!(!can_split(&[king(), queen(), ten()]));
    }

    #[test]
    fn dealer_hits_below_17() {
        assert!(dealer_must_hit(&[ten(), five()]));
    }

    #[test]
    fn dealer_stands_on_soft_17_under_house_rule() {
        assert!(!dealer_must_hit(&[
            ace(),
            c(CardRank::Number(6), CardSuit::Clubs)
        ]));
    }

    #[test]
    fn dealer_stands_on_hard_17() {
        assert!(!dealer_must_hit(&[ten(), seven()]));
    }

    #[test]
    fn bet_rejects_out_of_range() {
        assert_eq!(Bet::new(9), Err(BetError::BelowMin));
        assert_eq!(Bet::new(101), Err(BetError::AboveMax));
        assert!(Bet::new(10).is_ok());
        assert!(Bet::new(100).is_ok());
    }

    #[test]
    fn settle_player_bust_loses_even_if_dealer_also_busts() {
        let outcome = settle(&[king(), queen(), five()], &[king(), queen(), nine()]);
        assert_eq!(outcome, Outcome::DealerWin);
    }

    #[test]
    fn settle_both_naturals_is_push() {
        assert_eq!(settle(&[ace(), king()], &[ace(), queen()]), Outcome::Push);
    }

    #[test]
    fn settle_player_natural_beats_dealer_21_of_three_cards() {
        let outcome = settle(
            &[ace(), king()],
            &[five(), five(), c(CardRank::Number(2), CardSuit::Clubs)],
        );
        assert_eq!(outcome, Outcome::PlayerBlackjack);
    }

    #[test]
    fn settle_higher_total_wins() {
        let outcome = settle(&[ten(), nine()], &[ten(), seven()]);
        assert_eq!(outcome, Outcome::PlayerWin);
    }

    #[test]
    fn payout_credit_rounds_blackjack_bonus_toward_zero() {
        assert_eq!(
            payout_credit(Bet::new(25).unwrap(), Outcome::PlayerBlackjack),
            62
        );
    }

    #[test]
    fn shoe_draws_top_card() {
        let mut shoe = Shoe::from_top(vec![ten(), ace()]);
        assert_eq!(shoe.draw(), ten());
        assert_eq!(shoe.draw(), ace());
    }
}
