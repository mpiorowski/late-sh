use uuid::Uuid;

use super::*;
use crate::app::games::cards::CardSuit::{Clubs, Hearts, Spades};
use crate::app::lobby::daily::{
    cribbage::CribbageMove,
    std_deck::{self, Rank},
};

const SEAT_0: Uuid = Uuid::from_u128(1);
const SEAT_1: Uuid = Uuid::from_u128(2);

const PONE: [Card; 6] = [
    Card::new(Rank::Ace, Hearts),
    Card::new(Rank::Two, Hearts),
    Card::new(Rank::Three, Hearts),
    Card::new(Rank::Four, Hearts),
    Card::new(Rank::Six, Hearts),
    Card::new(Rank::Seven, Hearts),
];
const DEALER: [Card; 6] = [
    Card::new(Rank::Ace, Clubs),
    Card::new(Rank::Two, Clubs),
    Card::new(Rank::Three, Clubs),
    Card::new(Rank::Four, Clubs),
    Card::new(Rank::Six, Clubs),
    Card::new(Rank::Seven, Clubs),
];
const STARTER: Card = Card::new(Rank::Nine, Spades);

/// Seat 0 deals: seat 1 holds hearts, seat 0 clubs, the 9♠ is the starter.
fn state() -> DailyCribbageState {
    let mut deck: Vec<Card> = PONE.into_iter().chain(DEALER).collect();
    deck.push(STARTER);
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
    DailyCribbageState::parse(&value).expect("a rigged deal parses")
}

fn rendered(state: &DailyCribbageState, my_seat: usize, spectating: bool) -> String {
    table_lines(
        &state.table(),
        my_seat,
        None,
        &[],
        spectating,
        Tier::Full,
        String::new(),
    )
    .iter()
    .flat_map(|line| line.spans.iter())
    .map(|span| span.content.to_string())
    .collect()
}

fn shows(text: &str, cards: &[Card]) -> Vec<String> {
    cards
        .iter()
        .map(|card| card.label())
        .filter(|label| text.contains(label.as_str()))
        .collect()
}

#[test]
fn the_board_draws_your_hand_and_never_theirs() {
    let state = state();

    let pone = rendered(&state, 1, false);
    assert_eq!(shows(&pone, &PONE).len(), 6, "own hand is face up");
    assert!(
        shows(&pone, &DEALER).is_empty(),
        "the dealer's cards leaked"
    );
    assert!(
        !pone.contains(&STARTER.label()),
        "the starter shows before the cut"
    );

    let dealer = rendered(&state, 0, false);
    assert_eq!(shows(&dealer, &DEALER).len(), 6, "own hand is face up");
    assert!(shows(&dealer, &PONE).is_empty(), "pone's cards leaked");
}

#[test]
fn a_spectator_sees_neither_hand_nor_the_crib_but_does_see_the_cut() {
    let mut state = state();
    for discard in [[PONE[4], PONE[5]], [DEALER[4], DEALER[5]]] {
        state
            .apply_move(CribbageMove::Discard(discard))
            .expect("the discard is legal");
    }

    let watched = rendered(&state, 0, true);
    assert!(
        shows(&watched, &PONE).is_empty(),
        "spectator saw pone's hand"
    );
    assert!(
        shows(&watched, &DEALER).is_empty(),
        "spectator saw the dealer's hand"
    );
    assert!(watched.contains(&STARTER.label()), "the cut is public");

    // The crib stays face down for the players too until the show.
    let dealer = rendered(&state, 0, false);
    assert!(
        shows(&dealer, &[PONE[4], PONE[5]]).is_empty(),
        "the crib leaked"
    );
}

#[test]
fn the_card_keys_are_hinted_only_to_the_seat_that_owes_the_move() {
    let mut state = state();
    assert_eq!(
        move_hints(state.table().phase, 1),
        [
            ("arrows/wasd", "choose card"),
            ("Space/Enter", "pick for crib")
        ]
    );
    assert!(
        move_hints(state.table().phase, 0).is_empty(),
        "the dealer waits on pone's discard"
    );

    for discard in [[PONE[4], PONE[5]], [DEALER[4], DEALER[5]]] {
        state
            .apply_move(CribbageMove::Discard(discard))
            .expect("the discard is legal");
    }
    assert_eq!(
        move_hints(state.table().phase, 1),
        [("arrows/wasd", "choose card"), ("Space/Enter", "play")]
    );
    assert!(
        move_hints(state.table().phase, 0).is_empty(),
        "the dealer waits on pone's card"
    );
}
