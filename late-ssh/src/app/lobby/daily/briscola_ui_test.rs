use super::*;
use crate::app::lobby::daily::briscola::{DECK, Rank};

const SEAT_0: Uuid = Uuid::from_u128(1);
const SEAT_1: Uuid = Uuid::from_u128(2);

const fn card(rank: Rank, suit: CardSuit) -> Card {
    Card { suit, rank }
}

/// A rigged deal: `front` lands in deck order (three cards each, then the
/// trump), the rest of the deck behind it.
fn deal(front: &[Card]) -> DailyBriscolaState {
    let mut deck = front.to_vec();
    deck.extend(
        (0..DECK as u8)
            .filter_map(Card::from_id)
            .filter(|c| !front.contains(c)),
    );
    DailyBriscolaState {
        version: 1,
        revision: 0,
        seats: [SEAT_0, SEAT_1],
        deck,
        plays: Vec::new(),
    }
}

/// Seat 0 holds A♥ 2♦ 2♣, seat 1 holds 3♥ 4♦ 4♣, trump 2♠.
fn table_deal() -> DailyBriscolaState {
    deal(&[
        card(Rank::Ace, CardSuit::Hearts),
        card(Rank::Two, CardSuit::Diamonds),
        card(Rank::Two, CardSuit::Clubs),
        card(Rank::Three, CardSuit::Hearts),
        card(Rank::Four, CardSuit::Diamonds),
        card(Rank::Four, CardSuit::Clubs),
        card(Rank::Two, CardSuit::Spades),
    ])
}

fn rendered(state: &DailyBriscolaState, my_seat: usize, spectating: bool) -> String {
    let table = state.table();
    // The middle label needs a board to name players, so tests pass it blank;
    // the assertions here are about which cards are drawn.
    table_lines(
        state,
        &table,
        my_seat,
        None,
        spectating,
        Tier::Full,
        String::new(),
    )
    .iter()
    .flat_map(|line| line.spans.iter())
    .map(|span| span.content.to_string())
    .collect()
}

#[test]
fn the_board_draws_your_hand_and_never_theirs() {
    let state = table_deal();

    let mine = rendered(&state, 0, false);
    for label in ["A♥", "2♦", "2♣"] {
        assert!(mine.contains(label), "own hand is face up: {label}");
    }
    for label in ["3♥", "4♦", "4♣"] {
        assert!(!mine.contains(label), "opponent's card leaked: {label}");
    }
    // The trump is turned up for both players from the deal.
    assert!(mine.contains("2♠"));

    // The same board from the other seat is the mirror image.
    let theirs = rendered(&state, 1, false);
    for label in ["3♥", "4♦", "4♣"] {
        assert!(theirs.contains(label), "own hand is face up: {label}");
    }
    for label in ["A♥", "2♦", "2♣"] {
        assert!(!theirs.contains(label), "opponent's card leaked: {label}");
    }
}

#[test]
fn a_spectator_sees_neither_hand() {
    let state = table_deal();

    let watched = rendered(&state, 0, true);

    for label in ["A♥", "2♦", "2♣", "3♥", "4♦", "4♣"] {
        assert!(!watched.contains(label), "spectator saw a card: {label}");
    }
    assert!(watched.contains("2♠"), "the trump stays public");
}

#[test]
fn the_table_shows_the_open_trick_then_the_one_just_taken() {
    let mut state = table_deal();

    state
        .apply_play(card(Rank::Ace, CardSuit::Hearts))
        .expect("seat 0 leads");
    // Mid-trick the led card is on the table, seen from either side.
    assert!(rendered(&state, 1, false).contains("A♥"));

    state
        .apply_play(card(Rank::Three, CardSuit::Hearts))
        .expect("seat 1 answers");
    // The trick is over, so both its cards stay up until the next lead: a
    // player coming back a day later needs to see what happened.
    let after = rendered(&state, 1, false);
    assert!(after.contains("A♥"), "the lead stays on the table");
    assert!(after.contains("3♥"), "the answer stays on the table");
}
