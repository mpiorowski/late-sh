use rand::{SeedableRng, rngs::StdRng};

use super::*;
use crate::app::games::cards::CardSuit::{Clubs, Diamonds, Hearts, Spades};

const SEAT_0: Uuid = Uuid::from_u128(1);
const SEAT_1: Uuid = Uuid::from_u128(2);

const fn c(rank: Rank, suit: crate::app::games::cards::CardSuit) -> Card {
    Card::new(rank, suit)
}

/// A rigged first hand: pone's six, the dealer's six, then the starter, the
/// rest of the deck behind them. Seat 0 deals, so seat 1 is pone.
fn rigged(pone: [Card; 6], dealer: [Card; 6], starter: Card) -> DailyCribbageState {
    let mut deck: Vec<Card> = pone.into_iter().chain(dealer).collect();
    deck.push(starter);
    let rest: Vec<Card> = std_deck::fresh_deck()
        .into_iter()
        .filter(|card| !deck.contains(card))
        .collect();
    deck.extend(rest);
    DailyCribbageState {
        version: STATE_VERSION,
        revision: 0,
        seats: [SEAT_0, SEAT_1],
        deals: vec![deck],
        moves: Vec::new(),
    }
}

fn play(state: &mut DailyCribbageState, card: Card) -> MoveOutcome {
    state
        .apply_move(CribbageMove::Play(card))
        .unwrap_or_else(|error| panic!("{} should play: {error}", card.label()))
}

fn discard(state: &mut DailyCribbageState, first: Card, second: Card) {
    state
        .apply_move(CribbageMove::Discard([first, second]))
        .expect("the discard is legal");
}

fn peg(seat: usize, points: u32, why: Why) -> Peg {
    Peg {
        hand: 0,
        seat,
        points,
        why,
    }
}

#[test]
fn the_best_hand_in_cribbage_counts_twenty_nine() {
    let count = count_hand(
        [
            c(Rank::Five, Hearts),
            c(Rank::Five, Diamonds),
            c(Rank::Five, Clubs),
            c(Rank::Jack, Spades),
        ],
        c(Rank::Five, Spades),
        false,
    );
    assert_eq!(
        count,
        Count {
            fifteens: 16,
            pairs: 12,
            runs: 0,
            flush: 0,
            nobs: 1,
        }
    );
    assert_eq!(count.total(), 29);
}

#[test]
fn a_double_run_scores_both_runs_and_the_pair() {
    // 3-4-4-5 with a king cut: two runs of three, the pair of fours, and
    // one fifteen (K+5).
    let count = count_hand(
        [
            c(Rank::Three, Hearts),
            c(Rank::Four, Hearts),
            c(Rank::Four, Diamonds),
            c(Rank::Five, Clubs),
        ],
        c(Rank::King, Spades),
        false,
    );
    assert_eq!(count.runs, 6);
    assert_eq!(count.pairs, 2);
    assert_eq!(count.fifteens, 2);
    assert_eq!(count.total(), 10);
}

#[test]
fn a_four_card_flush_counts_in_hand_but_not_in_the_crib() {
    let hearts = [
        c(Rank::Two, Hearts),
        c(Rank::Four, Hearts),
        c(Rank::Eight, Hearts),
        c(Rank::Queen, Hearts),
    ];
    assert_eq!(count_hand(hearts, c(Rank::King, Spades), false).flush, 4);
    assert_eq!(count_hand(hearts, c(Rank::King, Spades), true).flush, 0);
    assert_eq!(count_hand(hearts, c(Rank::King, Hearts), true).flush, 5);
}

#[test]
fn a_rigged_hand_pegs_and_counts_out_exactly() {
    let mut state = rigged(
        [
            c(Rank::Five, Hearts),
            c(Rank::Five, Diamonds),
            c(Rank::Ten, Clubs),
            c(Rank::Four, Spades),
            c(Rank::King, Spades),
            c(Rank::Queen, Spades),
        ],
        [
            c(Rank::Five, Clubs),
            c(Rank::Six, Hearts),
            c(Rank::Three, Diamonds),
            c(Rank::Two, Clubs),
            c(Rank::King, Hearts),
            c(Rank::Queen, Hearts),
        ],
        c(Rank::Nine, Spades),
    );

    // Pone discards first, then the dealer; the starter stays down until both have.
    assert_eq!(state.table().phase, Phase::Discard(1));
    discard(&mut state, c(Rank::King, Spades), c(Rank::Queen, Spades));
    assert_eq!(state.table().phase, Phase::Discard(0));
    assert_eq!(state.table().starter, None);
    discard(&mut state, c(Rank::King, Hearts), c(Rank::Queen, Hearts));
    assert_eq!(state.table().starter, Some(c(Rank::Nine, Spades)));
    assert_eq!(state.table().phase, Phase::Peg(1), "pone leads the pegging");

    play(&mut state, c(Rank::Five, Hearts));
    let pair = play(&mut state, c(Rank::Five, Clubs));
    assert_eq!(pair.label(), "5♣, a pair for 2");
    let royal = play(&mut state, c(Rank::Five, Diamonds));
    assert_eq!(royal.label(), "5♦, fifteen and pair royal for 8");
    play(&mut state, c(Rank::Six, Hearts));
    let thirty_one = play(&mut state, c(Rank::Ten, Clubs));
    assert_eq!(thirty_one.label(), "10♣, thirty-one for 2");
    assert_eq!(state.table().count, 0, "thirty-one starts a new count");
    assert_eq!(
        state.table().phase,
        Phase::Peg(0),
        "led by the player after the last card"
    );
    play(&mut state, c(Rank::Three, Diamonds));
    play(&mut state, c(Rank::Four, Spades));
    let last = play(&mut state, c(Rank::Two, Clubs));
    assert_eq!(
        last.label(),
        "2♣, a run of 3 and last card for 4, hand over"
    );

    let table = state.table();
    assert_eq!(table.phase, Phase::AwaitingDeal);
    assert_eq!(table.scores, [12, 16]);
    assert_eq!(
        table.log,
        vec![
            peg(0, 2, Why::Pair(2)),
            peg(1, 2, Why::Fifteen),
            peg(1, 6, Why::Pair(3)),
            peg(1, 2, Why::ThirtyOne),
            peg(0, 3, Why::Run(3)),
            peg(0, 1, Why::LastCard),
            peg(1, 6, Why::Show(ShowPart::PoneHand)),
            peg(0, 2, Why::Show(ShowPart::DealerHand)),
            peg(0, 4, Why::Show(ShowPart::Crib)),
        ]
    );
    let show = table.last_show.expect("the hand was shown");
    assert_eq!(
        show.parts
            .iter()
            .map(|part| (part.part, part.seat, part.count.total()))
            .collect::<Vec<_>>(),
        vec![
            (ShowPart::PoneHand, 1, 6),
            (ShowPart::DealerHand, 0, 2),
            (ShowPart::Crib, 0, 4),
        ]
    );
    assert_eq!(state.turn_user(), None, "nobody moves until the next deal");

    // The next deal passes the deal to seat 1, so seat 0 discards first.
    state
        .deal_next(&mut StdRng::seed_from_u64(7))
        .expect("the hand is over");
    let next = state.table();
    assert_eq!(
        (next.hand, next.dealer, next.phase),
        (1, 1, Phase::Discard(0))
    );
    assert_eq!(next.scores, [12, 16], "scores carry across hands");
    assert!(next.last_show.is_some(), "the last show stays readable");
}

#[test]
fn a_player_who_cannot_play_says_go_without_being_asked() {
    let mut state = rigged(
        [
            c(Rank::Ten, Hearts),
            c(Rank::Ten, Diamonds),
            c(Rank::Nine, Clubs),
            c(Rank::Ace, Spades),
            c(Rank::Two, Spades),
            c(Rank::Three, Spades),
        ],
        [
            c(Rank::King, Hearts),
            c(Rank::King, Diamonds),
            c(Rank::Queen, Clubs),
            c(Rank::Three, Hearts),
            c(Rank::Four, Spades),
            c(Rank::Six, Spades),
        ],
        c(Rank::Seven, Diamonds),
    );
    discard(&mut state, c(Rank::Two, Spades), c(Rank::Three, Spades));
    discard(&mut state, c(Rank::Four, Spades), c(Rank::Six, Spades));

    play(&mut state, c(Rank::Ten, Hearts));
    play(&mut state, c(Rank::King, Hearts));
    play(&mut state, c(Rank::Ten, Diamonds));
    // At 30 the dealer holds nothing small enough, so the play comes
    // straight back to pone, who makes 31 with the ace.
    assert_eq!(state.table().phase, Phase::Peg(1));
    let error = state
        .apply_move(CribbageMove::Play(c(Rank::Nine, Clubs)))
        .expect_err("39 is past 31");
    assert!(error.to_string().contains("past 31"));
    play(&mut state, c(Rank::Ace, Spades));

    play(&mut state, c(Rank::King, Diamonds));
    play(&mut state, c(Rank::Nine, Clubs));
    // Pone is out of cards and 3♥ does not fit on 29: go to the dealer.
    let go = play(&mut state, c(Rank::Queen, Clubs));
    assert_eq!(go.label(), "Q♣, go for 1");
    assert_eq!(state.table().count, 0);
    let last = play(&mut state, c(Rank::Three, Hearts));
    assert!(last.label().starts_with("3♥, last card for 1"));
}

#[test]
fn a_jack_cut_pegs_the_dealer_two_for_his_heels() {
    let mut state = rigged(
        [
            c(Rank::Ace, Hearts),
            c(Rank::Two, Hearts),
            c(Rank::Three, Hearts),
            c(Rank::Four, Hearts),
            c(Rank::Six, Hearts),
            c(Rank::Seven, Hearts),
        ],
        [
            c(Rank::Ace, Clubs),
            c(Rank::Two, Clubs),
            c(Rank::Three, Clubs),
            c(Rank::Four, Clubs),
            c(Rank::Six, Clubs),
            c(Rank::Seven, Clubs),
        ],
        c(Rank::Jack, Spades),
    );
    discard(&mut state, c(Rank::Six, Hearts), c(Rank::Seven, Hearts));
    discard(&mut state, c(Rank::Six, Clubs), c(Rank::Seven, Clubs));
    let table = state.table();
    assert_eq!(table.scores, [2, 0]);
    assert_eq!(table.log, vec![peg(0, 2, Why::HisHeels)]);
}

#[test]
fn moves_out_of_phase_or_not_held_are_rejected_and_not_recorded() {
    let mut state = rigged(
        [
            c(Rank::Ace, Hearts),
            c(Rank::Two, Hearts),
            c(Rank::Three, Hearts),
            c(Rank::Four, Hearts),
            c(Rank::Six, Hearts),
            c(Rank::Seven, Hearts),
        ],
        [
            c(Rank::Ace, Clubs),
            c(Rank::Two, Clubs),
            c(Rank::Three, Clubs),
            c(Rank::Four, Clubs),
            c(Rank::Six, Clubs),
            c(Rank::Seven, Clubs),
        ],
        c(Rank::Nine, Spades),
    );
    let errors = [
        CribbageMove::Play(c(Rank::Ace, Hearts)),
        CribbageMove::Discard([c(Rank::Ace, Clubs), c(Rank::Two, Clubs)]),
        CribbageMove::Discard([c(Rank::Ace, Hearts), c(Rank::Ace, Hearts)]),
    ]
    .map(|played| state.apply_move(played).expect_err("illegal").to_string());
    assert_eq!(
        errors,
        [
            "discard two cards to the crib first".to_string(),
            "that card is not in your hand".to_string(),
            "pick two different cards for the crib".to_string(),
        ]
    );
    assert!(state.moves.is_empty());
}

#[test]
fn a_whole_match_plays_to_61_and_every_point_is_logged() {
    let mut rng = StdRng::seed_from_u64(42);
    let mut state = DailyCribbageState::new(SEAT_0, SEAT_1, &mut rng);
    let winner = loop {
        let table = state.table();
        match table.phase {
            Phase::Discard(seat) => {
                let hand = &table.hands[seat];
                discard(&mut state, hand[0], hand[1]);
            }
            Phase::Peg(seat) => {
                let card = *table.hands[seat]
                    .iter()
                    .find(|card| table.count + card.value() <= MAX_COUNT)
                    .expect("the seat on the move can play");
                play(&mut state, card);
            }
            Phase::AwaitingDeal => state.deal_next(&mut rng).expect("the hand is over"),
            Phase::Won(seat) => break seat,
        }
    };

    let table = state.table();
    assert!(table.scores[winner] >= WINNING_SCORE);
    assert!(table.scores[other(winner)] < WINNING_SCORE);
    for seat in 0..2 {
        let logged: u32 = table
            .log
            .iter()
            .filter(|peg| peg.seat == seat)
            .map(|peg| peg.points)
            .sum();
        assert_eq!(logged, table.scores[seat]);
    }
    assert_eq!(state.turn_user(), None);
    let over = state
        .apply_move(CribbageMove::Play(c(Rank::Ace, Hearts)))
        .expect_err("nothing plays after the match is won");
    assert_eq!(over.to_string(), "the match is over");

    // What was stored reads back as the same match.
    let reread = DailyCribbageState::parse(&serde_json::to_value(&state).expect("serializes"))
        .expect("a played match parses");
    assert_eq!(reread.table(), table);
}

#[test]
fn a_history_that_does_not_replay_is_refused_at_parse() {
    let mut state = rigged(
        [
            c(Rank::Ace, Hearts),
            c(Rank::Two, Hearts),
            c(Rank::Three, Hearts),
            c(Rank::Four, Hearts),
            c(Rank::Six, Hearts),
            c(Rank::Seven, Hearts),
        ],
        [
            c(Rank::Ace, Clubs),
            c(Rank::Two, Clubs),
            c(Rank::Three, Clubs),
            c(Rank::Four, Clubs),
            c(Rank::Six, Clubs),
            c(Rank::Seven, Clubs),
        ],
        c(Rank::Nine, Spades),
    );
    state.moves.push(CribbageMove::Play(c(Rank::Ace, Hearts)));
    assert!(DailyCribbageState::parse(&serde_json::to_value(&state).expect("serializes")).is_err());
}
