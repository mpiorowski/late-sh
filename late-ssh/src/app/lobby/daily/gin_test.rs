use rand::{SeedableRng, rngs::StdRng};

use super::*;
use crate::app::games::cards::CardSuit::{self, Clubs, Diamonds, Hearts, Spades};
use crate::app::lobby::daily::std_deck::Rank;

const SEAT_0: Uuid = Uuid::from_u128(1);
const SEAT_1: Uuid = Uuid::from_u128(2);

const fn c(rank: Rank, suit: CardSuit) -> Card {
    Card::new(rank, suit)
}

/// A rigged first hand: pone's ten, the dealer's ten, the upcard, the head
/// of the stock, then the rest of the deck. Seat 0 deals, so seat 1 plays
/// first.
fn rigged(pone: [Card; 10], dealer: [Card; 10], upcard: Card, stock_top: Card) -> DailyGinState {
    let mut deck: Vec<Card> = pone.into_iter().chain(dealer).collect();
    deck.extend([upcard, stock_top]);
    let rest: Vec<Card> = std_deck::fresh_deck()
        .into_iter()
        .filter(|card| !deck.contains(card))
        .collect();
    deck.extend(rest);
    DailyGinState {
        version: STATE_VERSION,
        revision: 0,
        seats: [SEAT_0, SEAT_1],
        deals: vec![deck],
        moves: Vec::new(),
    }
}

/// Every card of pone's melded hand below except the one swapped per test.
const RUN: [Card; 3] = [
    c(Rank::Three, Hearts),
    c(Rank::Four, Hearts),
    c(Rank::Five, Hearts),
];
const EIGHTS: [Card; 3] = [
    c(Rank::Eight, Spades),
    c(Rank::Eight, Diamonds),
    c(Rank::Eight, Clubs),
];
const CLUBS: [Card; 3] = [
    c(Rank::Jack, Clubs),
    c(Rank::Queen, Clubs),
    c(Rank::King, Clubs),
];

fn pone_with(last: Card) -> [Card; 10] {
    [
        RUN[0], RUN[1], RUN[2], EIGHTS[0], EIGHTS[1], EIGHTS[2], CLUBS[0], CLUBS[1], CLUBS[2], last,
    ]
}

fn apply(state: &mut DailyGinState, played: GinMove) -> MoveOutcome {
    state
        .apply_move(played)
        .unwrap_or_else(|error| panic!("{played:?} should be legal: {error}"))
}

#[test]
fn melds_are_found_even_where_a_set_and_a_run_share_a_card() {
    let hand = [
        c(Rank::Seven, Hearts),
        c(Rank::Eight, Hearts),
        c(Rank::Nine, Hearts),
        c(Rank::Seven, Spades),
        c(Rank::Seven, Diamonds),
        c(Rank::Seven, Clubs),
        c(Rank::King, Spades),
        c(Rank::King, Diamonds),
        c(Rank::King, Clubs),
        c(Rank::Two, Spades),
    ];
    let melding = best_melding(&hand);
    assert_eq!(melding.deadwood, vec![c(Rank::Two, Spades)]);
    assert_eq!(melding.deadwood_points(), 2);
    assert_eq!(
        melding.melds,
        vec![
            vec![
                c(Rank::Seven, Hearts),
                c(Rank::Eight, Hearts),
                c(Rank::Nine, Hearts)
            ],
            vec![
                c(Rank::Seven, Diamonds),
                c(Rank::Seven, Clubs),
                c(Rank::Seven, Spades)
            ],
            vec![
                c(Rank::King, Diamonds),
                c(Rank::King, Clubs),
                c(Rank::King, Spades)
            ],
        ]
    );
    // Melds first, then deadwood: the order the board draws and the cursor walks.
    assert_eq!(melding.cards().last(), Some(&c(Rank::Two, Spades)));
}

#[test]
fn a_knock_scores_the_difference_after_the_defender_lays_off() {
    let mut state = rigged(
        pone_with(c(Rank::Ace, Diamonds)),
        [
            c(Rank::Six, Hearts),
            c(Rank::Seven, Hearts),
            c(Rank::Nine, Spades),
            c(Rank::Nine, Diamonds),
            c(Rank::Two, Diamonds),
            c(Rank::Four, Clubs),
            c(Rank::Five, Clubs),
            c(Rank::Ten, Diamonds),
            c(Rank::King, Diamonds),
            c(Rank::Queen, Hearts),
        ],
        c(Rank::Three, Spades),
        c(Rank::Two, Clubs),
    );
    assert_eq!(
        state.table().phase,
        Phase::Draw(1),
        "the non-dealer plays first"
    );
    let drew = apply(&mut state, GinMove::Draw(Pile::Stock));
    assert_eq!(drew.label(), "drew from the stock");
    assert_eq!(state.table().phase, Phase::Discard(1));

    let knock = apply(
        &mut state,
        GinMove::Discard {
            card: c(Rank::Two, Clubs),
            knock: true,
        },
    );
    // The defender lays 6♥ and then 7♥ off on the 3-5♥ run: 59 left against 1.
    assert_eq!(knock.label(), "knocked on 2♣ for 58");
    let table = state.table();
    assert_eq!(table.scores, [0, 58]);
    assert_eq!(table.phase, Phase::AwaitingDeal);
    let result = &table.results[0];
    assert_eq!((result.end, result.scored), (HandEnd::Knock, Some((1, 58))));
    assert_eq!(result.reveals[0].deadwood, vec![c(Rank::Ace, Diamonds)]);
    assert_eq!(
        result.reveals[1].laid_off,
        vec![c(Rank::Six, Hearts), c(Rank::Seven, Hearts)]
    );
    assert_eq!(result.reveals[1].deadwood_points(), 59);

    // The deal passes to seat 1, so seat 0 plays first in the next hand.
    state
        .deal_next(&mut StdRng::seed_from_u64(3))
        .expect("the hand is over");
    let next = state.table();
    assert_eq!((next.hand, next.dealer, next.phase), (1, 1, Phase::Draw(0)));
    assert_eq!(next.scores, [0, 58]);
}

#[test]
fn a_defender_level_with_the_knocker_undercuts_for_the_bonus() {
    let mut state = rigged(
        pone_with(c(Rank::Five, Diamonds)),
        [
            c(Rank::Nine, Spades),
            c(Rank::Nine, Diamonds),
            c(Rank::Nine, Clubs),
            c(Rank::Ten, Spades),
            c(Rank::Jack, Spades),
            c(Rank::Queen, Spades),
            c(Rank::Ace, Diamonds),
            c(Rank::Two, Diamonds),
            c(Rank::Three, Diamonds),
            c(Rank::Five, Spades),
        ],
        c(Rank::Three, Spades),
        c(Rank::Two, Clubs),
    );
    apply(&mut state, GinMove::Draw(Pile::Stock));
    let knock = apply(
        &mut state,
        GinMove::Discard {
            card: c(Rank::Two, Clubs),
            knock: true,
        },
    );
    assert_eq!(knock.label(), "knocked on 2♣, undercut for 25");
    assert_eq!(state.table().scores, [25, 0]);
}

#[test]
fn gin_scores_the_bonus_and_the_defender_may_not_lay_off() {
    let mut state = rigged(
        [
            RUN[0],
            RUN[1],
            RUN[2],
            c(Rank::Six, Hearts),
            EIGHTS[0],
            EIGHTS[1],
            EIGHTS[2],
            CLUBS[0],
            CLUBS[1],
            CLUBS[2],
        ],
        [
            c(Rank::Seven, Hearts),
            c(Rank::Nine, Spades),
            c(Rank::Nine, Diamonds),
            c(Rank::Two, Diamonds),
            c(Rank::Four, Clubs),
            c(Rank::Five, Clubs),
            c(Rank::Ten, Diamonds),
            c(Rank::King, Diamonds),
            c(Rank::Queen, Hearts),
            c(Rank::Ace, Spades),
        ],
        c(Rank::Three, Spades),
        c(Rank::Two, Clubs),
    );
    apply(&mut state, GinMove::Draw(Pile::Stock));
    let gin = apply(
        &mut state,
        GinMove::Discard {
            card: c(Rank::Two, Clubs),
            knock: true,
        },
    );
    // 7♥ would extend the run, but gin allows no layoffs: all 67 count.
    assert_eq!(gin.label(), "gin on 2♣ for 92");
    let table = state.table();
    let result = &table.results[0];
    assert_eq!(result.end, HandEnd::Gin);
    assert!(result.reveals[1].laid_off.is_empty());
    assert_eq!(table.scores, [0, 92]);
}

#[test]
fn illegal_moves_are_rejected_and_not_recorded() {
    let mut state = rigged(
        pone_with(c(Rank::King, Hearts)),
        [
            c(Rank::Seven, Hearts),
            c(Rank::Nine, Spades),
            c(Rank::Nine, Diamonds),
            c(Rank::Two, Diamonds),
            c(Rank::Four, Clubs),
            c(Rank::Five, Clubs),
            c(Rank::Ten, Diamonds),
            c(Rank::King, Diamonds),
            c(Rank::Queen, Hearts),
            c(Rank::Ace, Spades),
        ],
        c(Rank::Three, Spades),
        c(Rank::Two, Clubs),
    );
    let early = state
        .apply_move(GinMove::Discard {
            card: c(Rank::King, Hearts),
            knock: false,
        })
        .expect_err("a discard needs a draw first");
    assert_eq!(early.to_string(), "draw a card first");

    let took = apply(&mut state, GinMove::Draw(Pile::Discard));
    assert_eq!(took.label(), "took 3♠");
    let errors = [
        GinMove::Discard {
            card: c(Rank::Three, Spades),
            knock: false,
        },
        // Breaking the run leaves 3♥ 4♥ K♥ 3♠ out.
        GinMove::Discard {
            card: c(Rank::Five, Hearts),
            knock: true,
        },
        GinMove::Draw(Pile::Stock),
    ]
    .map(|played| state.apply_move(played).expect_err("illegal").to_string());
    assert_eq!(
        errors,
        [
            "you cannot throw back the card you just took".to_string(),
            "you need 10 or less deadwood to knock, you hold 20".to_string(),
            "you have already drawn".to_string(),
        ]
    );
    assert_eq!(state.moves.len(), 1, "only the draw was recorded");
}

#[test]
fn a_hand_nobody_knocks_goes_dead_with_two_cards_in_the_stock() {
    let mut state = DailyGinState::new(SEAT_0, SEAT_1, &mut StdRng::seed_from_u64(9));
    let mut turns = 0;
    while let Phase::Draw(seat) = state.table().phase {
        apply(&mut state, GinMove::Draw(Pile::Stock));
        let drawn = *state.table().hands[seat].last().expect("just drew");
        apply(
            &mut state,
            GinMove::Discard {
                card: drawn,
                knock: false,
            },
        );
        turns += 1;
    }
    let table = state.table();
    assert_eq!(turns, STOCK - DEAD_STOCK);
    assert_eq!(table.phase, Phase::AwaitingDeal);
    assert_eq!(table.stock_remaining(), DEAD_STOCK);
    assert_eq!(table.results[0].end, HandEnd::Dead);
    assert_eq!(table.results[0].scored, None);
    assert_eq!(table.scores, [0, 0]);
}

#[test]
fn a_whole_match_plays_to_100() {
    let mut rng = StdRng::seed_from_u64(11);
    let mut state = DailyGinState::new(SEAT_0, SEAT_1, &mut rng);
    let winner = loop {
        let table = state.table();
        assert!(table.hand < 200, "the match never ended");
        match table.phase {
            Phase::Draw(_) => {
                apply(&mut state, GinMove::Draw(Pile::Stock));
            }
            Phase::Discard(seat) => {
                // Throw whichever card leaves the least deadwood, and knock
                // the moment that is allowed.
                let hand = &table.hands[seat];
                let card = *hand
                    .iter()
                    .min_by_key(|card| deadwood_after_discard(hand, **card))
                    .expect("eleven cards in hand");
                let knock = deadwood_after_discard(hand, card) <= MAX_KNOCK;
                apply(&mut state, GinMove::Discard { card, knock });
            }
            Phase::AwaitingDeal => state.deal_next(&mut rng).expect("the hand is over"),
            Phase::Won(seat) => break seat,
        }
    };
    let table = state.table();
    assert!(table.scores[winner] >= TARGET_SCORE);
    assert!(table.scores[other(winner)] < TARGET_SCORE);
    let scored: [u32; 2] = table.results.iter().fold([0, 0], |mut totals, result| {
        if let Some((seat, points)) = result.scored {
            totals[seat] += points;
        }
        totals
    });
    assert_eq!(scored, table.scores, "every point comes from a hand result");
    assert_eq!(state.turn_user(), None);

    let reread = DailyGinState::parse(&serde_json::to_value(&state).expect("serializes"))
        .expect("a played match parses");
    assert_eq!(reread.table(), table);
}
