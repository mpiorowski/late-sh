use crate::app::games::cards::{CardRank, CardSuit, PlayingCard};
use crate::app::lobby::house::blackjack::state::*;

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
