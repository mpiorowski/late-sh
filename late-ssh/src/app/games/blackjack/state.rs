use serde::{Deserialize, Serialize};

use crate::app::games::cards::{CardRank, PlayingCard};

pub const MIN_BET: i64 = 10;
pub const MAX_BET: i64 = 100;
pub const BLACKJACK_TARGET: u8 = 21;
pub const DEALER_STAND_ON: u8 = 17;

// House rule: dealer stands on soft 17. Flip to false later if we want S17/H17
// as a table variant.
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

// Splits are allowed on any two cards of equal point value, so K+Q counts.
// If we ever restrict to same-rank only, this is the single place to change.
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

// Chips to credit back to the player. Loss returns 0, push refunds the bet,
// win returns 2x (bet + 1x winnings), natural blackjack returns 2.5x (bet +
// 1.5x winnings) rounded toward zero.
pub fn payout_credit(bet: Bet, outcome: Outcome) -> i64 {
    let b = bet.amount();
    match outcome {
        Outcome::DealerWin => 0,
        Outcome::Push => b,
        Outcome::PlayerWin => b * 2,
        Outcome::PlayerBlackjack => b * 2 + b / 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::games::cards::{CardRank, CardSuit, PlayingCard};

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
    fn five() -> PlayingCard {
        c(CardRank::Number(5), CardSuit::Hearts)
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
        assert!(!dealer_must_hit(&[ace(), c(CardRank::Number(6), CardSuit::Clubs)]));
    }

    #[test]
    fn dealer_stands_on_hard_17() {
        assert!(!dealer_must_hit(&[ten(), c(CardRank::Number(7), CardSuit::Clubs)]));
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
        let outcome = settle(&[ace(), king()], &[five(), five(), c(CardRank::Number(2), CardSuit::Clubs)]);
        assert_eq!(outcome, Outcome::PlayerBlackjack);
    }

    #[test]
    fn settle_higher_total_wins() {
        let outcome = settle(&[ten(), nine()], &[ten(), c(CardRank::Number(8), CardSuit::Clubs)]);
        assert_eq!(outcome, Outcome::PlayerWin);
    }

    #[test]
    fn payout_credit_values() {
        let bet = Bet::new(20).unwrap();
        assert_eq!(payout_credit(bet, Outcome::DealerWin), 0);
        assert_eq!(payout_credit(bet, Outcome::Push), 20);
        assert_eq!(payout_credit(bet, Outcome::PlayerWin), 40);
        assert_eq!(payout_credit(bet, Outcome::PlayerBlackjack), 50);
    }

    #[test]
    fn blackjack_payout_rounds_toward_zero_on_odd_bets() {
        let bet = Bet::new(15).unwrap();
        assert_eq!(payout_credit(bet, Outcome::PlayerBlackjack), 37);
    }
}
