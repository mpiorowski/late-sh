use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast::{self, error::TryRecvError};
use uuid::Uuid;

use crate::app::games::{
    blackjack::svc::{BlackjackEvent, BlackjackService},
    cards::{CardRank, CardSuit, PlayingCard},
};

pub const MIN_BET: i64 = 10;
pub const MAX_BET: i64 = 100;
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
        if amount < MIN_BET {
            return Err(BetError::BelowMin);
        }
        if amount > MAX_BET {
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
    pub dealer_hand: Vec<PlayingCard>,
    pub player_hand: Vec<PlayingCard>,
    pub current_bet_amount: Option<i64>,
    pub phase: Phase,
    pub last_outcome: Option<Outcome>,
    pub last_net_change: i64,
    pub bet_input: String,
    pub status_message: String,
    pub dealer_revealed: bool,
    pub dealer_score: Option<HandScore>,
    pub player_score: Option<HandScore>,
    pub outcome_banner: Option<(String, String)>,
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

    fn draw(&mut self) -> PlayingCard {
        if self.cards.len() <= self.penetration {
            self.cards = fresh_shoe();
            shuffle(&mut self.cards);
        }
        self.cards.pop().expect("shoe should never be empty")
    }

    pub fn remaining(&self) -> usize {
        self.cards.len()
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

pub struct State {
    room_id: Uuid,
    user_id: Uuid,
    pub(crate) balance: i64,
    pub(crate) shoe: Shoe,
    pub(crate) dealer_hand: Vec<PlayingCard>,
    pub(crate) player_hand: Vec<PlayingCard>,
    pub(crate) bet: Option<Bet>,
    pub(crate) phase: Phase,
    pending_request_id: Option<Uuid>,
    pub(crate) last_outcome: Option<Outcome>,
    pub(crate) last_net_change: i64,
    pub(crate) bet_input: String,
    pub(crate) status_message: String,
    svc: Option<BlackjackService>,
    event_rx: broadcast::Receiver<BlackjackEvent>,
}

impl State {
    pub fn new(svc: BlackjackService, user_id: Uuid, balance: i64) -> Self {
        let event_rx = svc.subscribe_events();
        Self {
            room_id: Uuid::now_v7(),
            user_id,
            balance,
            shoe: Shoe::new(),
            dealer_hand: Vec::new(),
            player_hand: Vec::new(),
            bet: None,
            phase: Phase::Betting,
            pending_request_id: None,
            last_outcome: None,
            last_net_change: 0,
            bet_input: String::new(),
            status_message: format!("Place a bet ({MIN_BET}-{MAX_BET} chips)."),
            svc: Some(svc),
            event_rx,
        }
    }

    pub fn tick(&mut self) {
        loop {
            match self.event_rx.try_recv() {
                Ok(event) => self.apply_event(event),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Closed) => break,
                Err(TryRecvError::Lagged(skipped)) => {
                    self.status_message = format!("Blackjack updates lagged ({skipped} dropped).");
                }
            }
        }
    }

    pub fn append_bet_digit(&mut self, digit: char) {
        if self.phase != Phase::Betting || !digit.is_ascii_digit() {
            return;
        }
        if self.bet_input.len() < 3 {
            self.bet_input.push(digit);
        }
    }

    pub fn pop_bet_digit(&mut self) {
        if self.phase == Phase::Betting {
            self.bet_input.pop();
        }
    }

    pub fn submit_bet_from_buffer(&mut self) {
        if self.phase != Phase::Betting {
            return;
        }
        let Ok(amount) = self.bet_input.parse::<i64>() else {
            self.status_message = "Enter a bet first.".to_string();
            return;
        };
        self.submit_bet(amount);
    }

    pub fn submit_bet(&mut self, amount: i64) {
        if self.phase != Phase::Betting {
            return;
        }
        let request_id = Uuid::now_v7();
        self.bet = Bet::new(amount).ok();
        self.pending_request_id = Some(request_id);
        self.phase = Phase::BetPending;
        self.status_message = format!("Placing bet: {amount} chips...");
        if let Some(svc) = &self.svc {
            svc.place_bet_task(self.room_id, self.user_id, request_id, amount);
        }
    }

    pub fn hit(&mut self) {
        if self.phase != Phase::PlayerTurn {
            return;
        }
        self.player_hand.push(self.shoe.draw());
        let total = score(&self.player_hand).total;
        if is_bust(&self.player_hand) {
            self.finish_hand(Outcome::DealerWin);
        } else {
            self.status_message = format!("You hit. Total: {total}.");
        }
    }

    pub fn stand(&mut self) {
        if self.phase != Phase::PlayerTurn {
            return;
        }
        self.phase = Phase::DealerTurn;
        self.status_message = "Dealer's turn.".to_string();
        self.run_dealer();
    }

    pub fn next_hand(&mut self) {
        self.bet = None;
        self.dealer_hand.clear();
        self.player_hand.clear();
        self.last_outcome = None;
        self.last_net_change = 0;
        self.pending_request_id = None;
        self.phase = Phase::Betting;
        self.bet_input.clear();
        self.status_message = format!("Place a bet ({MIN_BET}-{MAX_BET} chips).");
    }

    pub fn current_bet_amount(&self) -> Option<i64> {
        self.bet.map(Bet::amount)
    }

    pub fn snapshot(&self) -> BlackjackSnapshot {
        BlackjackSnapshot {
            balance: self.balance,
            dealer_hand: self.dealer_hand.clone(),
            player_hand: self.player_hand.clone(),
            current_bet_amount: self.current_bet_amount(),
            phase: self.phase,
            last_outcome: self.last_outcome,
            last_net_change: self.last_net_change,
            bet_input: self.bet_input.clone(),
            status_message: self.status_message.clone(),
            dealer_revealed: self.dealer_revealed(),
            dealer_score: self.dealer_score(),
            player_score: self.player_score(),
            outcome_banner: self.outcome_banner(),
        }
    }

    pub fn dealer_score(&self) -> Option<HandScore> {
        if self.dealer_revealed() {
            Some(score(&self.dealer_hand))
        } else {
            None
        }
    }

    pub fn player_score(&self) -> Option<HandScore> {
        if self.player_hand.is_empty() {
            None
        } else {
            Some(score(&self.player_hand))
        }
    }

    pub fn dealer_revealed(&self) -> bool {
        matches!(self.phase, Phase::DealerTurn | Phase::Settling)
    }

    pub fn outcome_banner(&self) -> Option<(String, String)> {
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

    fn apply_event(&mut self, event: BlackjackEvent) {
        match event {
            BlackjackEvent::BetPlaced {
                room_id,
                user_id,
                request_id,
                result,
            } => {
                if room_id != self.room_id
                    || user_id != self.user_id
                    || Some(request_id) != self.pending_request_id
                {
                    return;
                }
                self.pending_request_id = None;
                match result {
                    Ok(new_balance) => {
                        self.balance = new_balance;
                        self.bet_input.clear();
                        self.deal_initial();
                    }
                    Err(message) => {
                        self.bet = None;
                        self.phase = Phase::Betting;
                        self.status_message = message;
                    }
                }
            }
            BlackjackEvent::HandSettled {
                room_id,
                user_id,
                new_balance,
                ..
            } => {
                if room_id == self.room_id && user_id == self.user_id {
                    self.balance = new_balance;
                }
            }
            BlackjackEvent::BetRefunded { .. } => {}
        }
    }

    fn deal_initial(&mut self) {
        self.player_hand.clear();
        self.dealer_hand.clear();
        self.last_outcome = None;
        self.last_net_change = 0;
        if self.bet.is_none() {
            self.bet = self
                .bet_input
                .parse::<i64>()
                .ok()
                .and_then(|amount| Bet::new(amount).ok());
        }

        self.player_hand.push(self.shoe.draw());
        self.dealer_hand.push(self.shoe.draw());
        self.player_hand.push(self.shoe.draw());
        self.dealer_hand.push(self.shoe.draw());

        let player_blackjack = is_natural_blackjack(&self.player_hand);
        let dealer_blackjack = is_natural_blackjack(&self.dealer_hand);
        if player_blackjack || dealer_blackjack {
            self.finish_hand(settle(&self.player_hand, &self.dealer_hand));
            return;
        }

        self.phase = Phase::PlayerTurn;
        self.status_message = "Hit or stand.".to_string();
    }

    fn run_dealer(&mut self) {
        while dealer_must_hit(&self.dealer_hand) {
            self.dealer_hand.push(self.shoe.draw());
        }
        let outcome = settle(&self.player_hand, &self.dealer_hand);
        self.finish_hand(outcome);
    }

    fn finish_hand(&mut self, outcome: Outcome) {
        let Some(bet) = self.bet else {
            self.phase = Phase::Betting;
            return;
        };
        let credit = payout_credit(bet, outcome);
        self.last_outcome = Some(outcome);
        self.last_net_change = credit - bet.amount();
        self.balance += credit;
        self.phase = Phase::Settling;
        self.status_message = match outcome {
            Outcome::PlayerBlackjack => "Blackjack pays 3:2.".to_string(),
            Outcome::PlayerWin => "You beat the dealer.".to_string(),
            Outcome::Push => "Push. Bet returned.".to_string(),
            Outcome::DealerWin if is_bust(&self.player_hand) => "You busted.".to_string(),
            Outcome::DealerWin => "Dealer takes the hand.".to_string(),
        };
        if let Some(svc) = &self.svc {
            svc.settle_hand_task(self.room_id, self.user_id, bet.amount(), outcome);
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
    fn eight() -> PlayingCard {
        c(CardRank::Number(8), CardSuit::Hearts)
    }
    fn seven() -> PlayingCard {
        c(CardRank::Number(7), CardSuit::Spades)
    }
    fn six() -> PlayingCard {
        c(CardRank::Number(6), CardSuit::Clubs)
    }
    fn five() -> PlayingCard {
        c(CardRank::Number(5), CardSuit::Hearts)
    }

    fn test_state(top_cards: Vec<PlayingCard>, balance: i64) -> State {
        let (_, event_rx) = broadcast::channel(8);
        State {
            room_id: Uuid::nil(),
            user_id: Uuid::nil(),
            balance,
            shoe: Shoe::from_top(top_cards),
            dealer_hand: Vec::new(),
            player_hand: Vec::new(),
            bet: None,
            phase: Phase::Betting,
            pending_request_id: None,
            last_outcome: None,
            last_net_change: 0,
            bet_input: String::new(),
            status_message: String::new(),
            svc: None,
            event_rx,
        }
    }

    #[test]
    fn ace_plus_king_is_soft_21() {
        let s = score(&[ace(), king()]);
        assert_eq!(s, HandScore { total: 21, soft: true });
    }

    #[test]
    fn pair_of_aces_is_soft_12() {
        let s = score(&[ace(), ace()]);
        assert_eq!(s, HandScore { total: 12, soft: true });
    }

    #[test]
    fn triple_ace_plus_nine_is_soft_21() {
        let s = score(&[ace(), ace(), nine()]);
        assert_eq!(s, HandScore { total: 21, soft: true });
    }

    #[test]
    fn ace_plus_ace_plus_king_is_hard_12() {
        let s = score(&[ace(), ace(), king()]);
        assert_eq!(s, HandScore { total: 12, soft: false });
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
        let outcome = settle(&[ten(), nine()], &[ten(), eight()]);
        assert_eq!(outcome, Outcome::PlayerWin);
    }

    #[test]
    fn payout_credit_rounds_blackjack_bonus_toward_zero() {
        assert_eq!(payout_credit(Bet::new(25).unwrap(), Outcome::PlayerBlackjack), 62);
    }

    #[test]
    fn hit_to_bust_transitions_to_settling() {
        let mut state = test_state(vec![ten()], 500);
        state.phase = Phase::PlayerTurn;
        state.bet = Some(Bet::new(50).unwrap());
        state.player_hand = vec![king(), queen()];
        state.hit();

        assert_eq!(state.phase, Phase::Settling);
        assert_eq!(state.last_outcome, Some(Outcome::DealerWin));
        assert_eq!(state.balance, 500);
    }

    #[test]
    fn natural_vs_natural_push_keeps_round_alive_until_next_hand() {
        let mut state = test_state(vec![ace(), ace(), king(), queen()], 400);
        state.bet_input = "50".to_string();
        state.deal_initial();

        assert_eq!(state.phase, Phase::Settling);
        assert_eq!(state.last_outcome, Some(Outcome::Push));
        assert_eq!(state.last_net_change, 0);
        assert_eq!(state.balance, 450);
    }

    #[test]
    fn dealer_stands_on_seventeen_loop() {
        let mut state = test_state(vec![], 350);
        state.bet = Some(Bet::new(50).unwrap());
        state.phase = Phase::DealerTurn;
        state.player_hand = vec![ten(), seven()];
        state.dealer_hand = vec![ten(), seven()];
        state.run_dealer();

        assert_eq!(state.dealer_hand.len(), 2);
        assert_eq!(state.last_outcome, Some(Outcome::Push));
    }

    #[test]
    fn ace_promotion_updates_live_totals() {
        let mut state = test_state(vec![nine()], 500);
        state.phase = Phase::PlayerTurn;
        state.bet = Some(Bet::new(50).unwrap());
        state.player_hand = vec![ace(), six()];

        assert_eq!(state.player_score().unwrap().total, 17);
        state.hit();
        assert_eq!(state.player_score().unwrap().total, 16);
        assert_eq!(state.phase, Phase::PlayerTurn);
    }
}
