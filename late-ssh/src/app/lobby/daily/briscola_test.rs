use super::*;

const SEAT_0: Uuid = Uuid::from_u128(1);
const SEAT_1: Uuid = Uuid::from_u128(2);

const fn card(rank: Rank, suit: CardSuit) -> Card {
    Card { suit, rank }
}

/// A rigged deal: `front` lands in deck order (three cards each, the trump,
/// then the head of the stock) and the rest of the deck follows behind it.
fn deal(front: &[Card]) -> DailyBriscolaState {
    let mut deck = front.to_vec();
    deck.extend(fresh_deck().into_iter().filter(|c| !front.contains(c)));
    DailyBriscolaState {
        version: STATE_VERSION,
        revision: 0,
        seats: [SEAT_0, SEAT_1],
        deck,
        plays: Vec::new(),
    }
}

/// Play to the last card, always leading with whatever sits first in hand.
fn play_out(state: &mut DailyBriscolaState) {
    while state.plays.len() < DECK {
        let table = state.table();
        let card = table.hands[table.turn][0];
        state.apply_play(card).expect("a held card is playable");
    }
}

#[test]
fn a_fresh_deal_is_the_whole_deck_dealt_once() {
    let state = DailyBriscolaState::new(SEAT_0, SEAT_1);

    assert_eq!(state.deck.len(), DECK);
    let mut ids: Vec<u8> = state.deck.iter().map(|card| card.id()).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), DECK, "the deal repeats a card");

    let table = state.table();
    assert_eq!(table.hands[0].len(), HAND);
    assert_eq!(table.hands[1].len(), HAND);
    assert_eq!(table.turn, 0, "seat 0 leads the first trick");
    assert_eq!(table.points, [0, 0]);
    assert_eq!(state.stock_remaining(), DRAWS);
    // Both players see the trump from the start, and it is nobody's card yet.
    assert!(!table.hands.iter().any(|hand| hand.contains(&state.trump())));
}

#[test]
fn the_follower_takes_the_trick_by_rank_or_by_trump() {
    let trump = CardSuit::Spades;
    let lead = card(Rank::King, CardSuit::Hearts);

    // Following suit: rank decides, and the briscola order is not the face
    // order: a three beats a king.
    assert!(takes_trick(
        card(Rank::Three, CardSuit::Hearts),
        lead,
        trump
    ));
    assert!(!takes_trick(
        card(Rank::Queen, CardSuit::Hearts),
        lead,
        trump
    ));
    // Off suit answers lose however big they are, unless they are trump.
    assert!(!takes_trick(
        card(Rank::Ace, CardSuit::Diamonds),
        lead,
        trump
    ));
    assert!(takes_trick(card(Rank::Two, trump), lead, trump));
    // A led trump only falls to a bigger trump.
    let trump_lead = card(Rank::King, trump);
    assert!(!takes_trick(
        card(Rank::Ace, CardSuit::Hearts),
        trump_lead,
        trump
    ));
    assert!(takes_trick(card(Rank::Ace, trump), trump_lead, trump));
}

#[test]
fn the_trick_winner_draws_first_and_leads_the_next_trick() {
    let mut state = deal(&[
        card(Rank::Ace, CardSuit::Hearts),
        card(Rank::Two, CardSuit::Diamonds),
        card(Rank::Two, CardSuit::Clubs),
        card(Rank::Three, CardSuit::Hearts),
        card(Rank::Four, CardSuit::Diamonds),
        card(Rank::Four, CardSuit::Clubs),
        card(Rank::Two, CardSuit::Spades),
        card(Rank::Five, CardSuit::Hearts),
        card(Rank::Six, CardSuit::Hearts),
    ]);

    state
        .apply_play(card(Rank::Ace, CardSuit::Hearts))
        .expect("seat 0 leads");
    let mid = state.table();
    assert_eq!(mid.turn, 1, "the follower is on the clock");
    assert_eq!(mid.led, Some((0, card(Rank::Ace, CardSuit::Hearts))));

    let outcome = state
        .apply_play(card(Rank::Three, CardSuit::Hearts))
        .expect("seat 1 answers");
    assert_eq!(
        outcome.trick,
        Some(TrickWin {
            winner: 0,
            points: 21
        })
    );

    let table = state.table();
    assert_eq!(table.turn, 0, "the winner leads next");
    assert_eq!(table.points, [21, 0]);
    assert_eq!(table.tricks, [1, 0]);
    // Winner draws first: seat 0 takes the top of the stock, seat 1 the next.
    assert_eq!(
        table.hands[0].last(),
        Some(&card(Rank::Five, CardSuit::Hearts))
    );
    assert_eq!(
        table.hands[1].last(),
        Some(&card(Rank::Six, CardSuit::Hearts))
    );
    assert_eq!(state.stock_remaining(), DRAWS - 2);

    // The next trick goes the other way, and the label follows the mover:
    // seat 1's answer takes this one.
    state
        .apply_play(card(Rank::Two, CardSuit::Diamonds))
        .expect("the winner leads the second trick");
    let outcome = state
        .apply_play(card(Rank::Four, CardSuit::Diamonds))
        .expect("seat 1 answers");
    assert_eq!(outcome.label(), "4♦, takes the trick");
    assert_eq!(state.table().turn, 1, "the new winner leads");
}

#[test]
fn the_trump_is_the_last_card_drawn() {
    let state = DailyBriscolaState::new(SEAT_0, SEAT_1);

    // The stock empties into the hands first; the turned-up trump lies under
    // it, so whoever loses the last draw takes it.
    assert_eq!(state.draw_at(0), Some(state.deck[STOCK_START]));
    assert_eq!(state.draw_at(DRAWS - 2), Some(state.deck[DECK - 1]));
    assert_eq!(state.draw_at(DRAWS - 1), Some(state.trump()));
    assert_eq!(state.draw_at(DRAWS), None);
}

#[test]
fn passing_sixty_settles_it_and_an_even_split_draws() {
    // Mid-match, nobody past 60: play on.
    assert_eq!(settle([45, 30], 30), None);
    // Past 60 with cards still out: the rest can no longer catch it.
    assert_eq!(settle([61, 14], 24), Some(MatchEnd::Winner(0)));
    assert_eq!(settle([14, 61], 24), Some(MatchEnd::Winner(1)));
    // The last trick played and the 120 split evenly.
    assert_eq!(settle([60, 60], DECK), Some(MatchEnd::Draw));
}

#[test]
fn taking_sixty_one_points_finishes_the_match() {
    // Three aces against three threes: 21 points a trick, so the third one
    // carries seat 0 past sixty.
    let mut state = deal(&[
        card(Rank::Ace, CardSuit::Hearts),
        card(Rank::Ace, CardSuit::Diamonds),
        card(Rank::Ace, CardSuit::Clubs),
        card(Rank::Three, CardSuit::Hearts),
        card(Rank::Three, CardSuit::Diamonds),
        card(Rank::Three, CardSuit::Clubs),
        card(Rank::Two, CardSuit::Spades),
    ]);

    for suit in [CardSuit::Hearts, CardSuit::Diamonds] {
        state
            .apply_play(card(Rank::Ace, suit))
            .expect("seat 0 leads");
        let outcome = state
            .apply_play(card(Rank::Three, suit))
            .expect("seat 1 answers");
        assert_eq!(outcome.finish, None, "still short of sixty-one");
    }

    state
        .apply_play(card(Rank::Ace, CardSuit::Clubs))
        .expect("seat 0 leads");
    let outcome = state
        .apply_play(card(Rank::Three, CardSuit::Clubs))
        .expect("seat 1 answers");

    assert_eq!(outcome.finish, Some(MatchEnd::Winner(0)));
    assert_eq!(outcome.totals, [63, 0]);
    // The label speaks from the mover's side: seat 1's answer lost the trick.
    assert_eq!(outcome.label(), "3♣, loses 21, 63-0");
}

#[test]
fn a_card_you_are_not_holding_is_rejected() {
    let mut state = deal(&[
        card(Rank::Ace, CardSuit::Hearts),
        card(Rank::Two, CardSuit::Diamonds),
        card(Rank::Two, CardSuit::Clubs),
        card(Rank::Three, CardSuit::Hearts),
        card(Rank::Four, CardSuit::Diamonds),
        card(Rank::Four, CardSuit::Clubs),
        card(Rank::Two, CardSuit::Spades),
    ]);

    // Seat 1's card, on seat 0's turn.
    let error = state
        .apply_play(card(Rank::Three, CardSuit::Hearts))
        .expect_err("that card is in the other hand");

    assert!(error.to_string().contains("not in your hand"));
    assert!(state.plays.is_empty(), "the rejected play is not recorded");
}

#[test]
fn playing_the_deck_out_accounts_for_every_card_and_point() {
    let mut state = DailyBriscolaState::new(SEAT_0, SEAT_1);
    play_out(&mut state);

    let table = state.table();
    assert_eq!(table.points[0] + table.points[1], TOTAL_POINTS);
    assert_eq!(table.tricks[0] + table.tricks[1], DECK / 2);
    assert!(table.hands.iter().all(Vec::is_empty), "cards left in hand");
    assert_eq!(table.drawn, DRAWS);
    assert_eq!(state.stock_remaining(), 0);
}
