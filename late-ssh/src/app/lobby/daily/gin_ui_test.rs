use uuid::Uuid;

use super::*;
use crate::app::games::cards::CardSuit::{Clubs, Diamonds, Hearts, Spades};
use crate::app::lobby::daily::std_deck::{self, Rank};

const SEAT_0: Uuid = Uuid::from_u128(1);
const SEAT_1: Uuid = Uuid::from_u128(2);

/// Seat 1 plays first (seat 0 deals). Every card is distinct across both
/// hands, the upcard and the top of the stock, so a label found in the
/// render can only have come from one place.
const PONE: [Card; 10] = [
    Card::new(Rank::Ace, Hearts),
    Card::new(Rank::Two, Hearts),
    Card::new(Rank::Three, Hearts),
    Card::new(Rank::Four, Hearts),
    Card::new(Rank::Five, Hearts),
    Card::new(Rank::Six, Hearts),
    Card::new(Rank::Seven, Hearts),
    Card::new(Rank::Eight, Hearts),
    Card::new(Rank::Nine, Hearts),
    Card::new(Rank::Ten, Hearts),
];
const DEALER: [Card; 10] = [
    Card::new(Rank::Ace, Clubs),
    Card::new(Rank::Two, Clubs),
    Card::new(Rank::Three, Clubs),
    Card::new(Rank::Four, Clubs),
    Card::new(Rank::Five, Clubs),
    Card::new(Rank::Six, Clubs),
    Card::new(Rank::Seven, Clubs),
    Card::new(Rank::Eight, Clubs),
    Card::new(Rank::Nine, Clubs),
    Card::new(Rank::Ten, Clubs),
];
const UPCARD: Card = Card::new(Rank::King, Spades);
const STOCK_TOP: Card = Card::new(Rank::Queen, Diamonds);

fn state() -> DailyGinState {
    let mut deck: Vec<Card> = PONE.into_iter().chain(DEALER).collect();
    deck.extend([UPCARD, STOCK_TOP]);
    let rest: Vec<Card> = std_deck::fresh_deck()
        .into_iter()
        .filter(|card| !deck.contains(card))
        .collect();
    deck.extend(rest);
    let value = serde_json::json!({
        "version": 1,
        "revision": 0,
        "seats": [SEAT_0, SEAT_1],
        "deals": [deck],
        "moves": [],
    });
    DailyGinState::parse(&value).expect("a rigged deal parses")
}

fn rendered(state: &DailyGinState, my_seat: usize, spectating: bool) -> String {
    table_lines(&state.table(), my_seat, None, None, spectating, Tier::Full)
        .iter()
        .flat_map(|line| line.spans.iter())
        .map(|span| span.content.to_string())
        .collect()
}

fn shown(text: &str, cards: &[Card]) -> usize {
    cards
        .iter()
        .filter(|card| text.contains(card.label().as_str()))
        .count()
}

#[test]
fn the_board_draws_your_hand_and_never_theirs() {
    let state = state();

    let pone = rendered(&state, 1, false);
    assert_eq!(shown(&pone, &PONE), 10, "own hand is face up");
    assert_eq!(shown(&pone, &DEALER), 0, "the dealer's cards leaked");
    assert!(pone.contains(&UPCARD.label()), "the upcard is public");
    assert!(!pone.contains(&STOCK_TOP.label()), "the stock leaked");

    let dealer = rendered(&state, 0, false);
    assert_eq!(shown(&dealer, &DEALER), 10, "own hand is face up");
    assert_eq!(shown(&dealer, &PONE), 0, "pone's cards leaked");
}

#[test]
fn a_card_drawn_from_the_stock_is_seen_by_the_drawer_alone() {
    let mut state = state();
    state
        .apply_move(GinMove::Draw(Pile::Stock))
        .expect("pone draws");

    assert!(rendered(&state, 1, false).contains(&STOCK_TOP.label()));
    assert!(!rendered(&state, 0, false).contains(&STOCK_TOP.label()));

    let watched = rendered(&state, 0, true);
    assert_eq!(shown(&watched, &PONE), 0, "spectator saw pone's hand");
    assert_eq!(
        shown(&watched, &DEALER),
        0,
        "spectator saw the dealer's hand"
    );
    assert!(
        !watched.contains(&STOCK_TOP.label()),
        "spectator saw the draw"
    );
    assert!(
        watched.contains(&UPCARD.label()),
        "the discard pile is public"
    );
}

#[test]
fn the_card_just_taken_from_the_pile_is_dimmed_because_it_cannot_go_back() {
    let mut state = state();
    state
        .apply_move(GinMove::Draw(Pile::Discard))
        .expect("pone takes the upcard");

    let lines = table_lines(&state.table(), 1, Some(0), None, false, Tier::Full);
    let faces = |card: Card| -> Vec<bool> {
        lines
            .iter()
            .flat_map(|line| line.spans.iter())
            .filter(|span| span.content.contains(card.label().as_str()))
            .map(|span| span.style.add_modifier.contains(Modifier::DIM))
            .collect()
    };
    assert_eq!(faces(UPCARD), vec![true], "the taken card reads as unplayable");
    assert!(
        faces(PONE[0]).iter().all(|dim| !dim),
        "every other card can be thrown"
    );
}
