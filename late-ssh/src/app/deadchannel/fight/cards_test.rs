use rand::{SeedableRng, rngs::StdRng};

use super::{Board, Card, DECK, DRAFTS, Effect, HAND, Piles, STATIC_CAP, deck, draft_owed};
use crate::app::deadchannel::fight::state::Powers;

fn count(cards: &[Card], card: Card) -> usize {
    cards.iter().filter(|held| **held == card).count()
}

/// The piles, wherever a card lies: nothing is lost or made by a turn.
fn all(piles: &Piles) -> Vec<Card> {
    piles
        .draw
        .iter()
        .chain(&piles.discard)
        .chain(piles.hand.iter().flatten())
        .copied()
        .collect()
}

#[test]
fn the_deck_is_ten_cards_and_a_deal_draws_five_of_them() {
    assert_eq!(
        (
            count(&DECK, Card::Strike),
            count(&DECK, Card::Block),
            count(&DECK, Card::Surge),
            count(&DECK, Card::Wipe),
            count(&DECK, Card::Static),
        ),
        (5, 3, 1, 1, 0)
    );

    let piles = Piles::deal(DECK.to_vec(), 2, &mut StdRng::seed_from_u64(4));
    assert_eq!(
        (
            piles.hand.iter().flatten().count(),
            piles.draw.len(),
            piles.discard.len()
        ),
        (HAND, 7, 0)
    );
    assert_eq!(piles.static_cards(), 2, "the day's static is dealt in");
    assert_eq!(
        Piles::deal(DECK.to_vec(), 2, &mut StdRng::seed_from_u64(4)),
        piles,
        "the same dice deal the same hand"
    );
    // A deck cannot be dealt with more static than it holds.
    assert_eq!(
        Piles::deal(DECK.to_vec(), 40, &mut StdRng::seed_from_u64(4)).static_cards(),
        STATIC_CAP
    );
}

/// Turn after turn the same ten cards go round: the hand to the discard
/// pile, the discard pile back under the draw pile when it runs dry.
#[test]
fn the_discard_pile_comes_back_round_when_the_draw_pile_runs_dry() {
    let mut rng = StdRng::seed_from_u64(9);
    let mut piles = Piles::deal(DECK.to_vec(), 0, &mut rng);
    for turn in 0..7 {
        piles.draw_hand(&mut rng);
        assert_eq!(
            piles.hand.iter().flatten().count(),
            HAND,
            "a full hand on turn {turn}"
        );
        let mut cards = all(&piles);
        cards.sort_by_key(|card| card.name());
        let mut deck = DECK.to_vec();
        deck.sort_by_key(|card| card.name());
        assert_eq!(cards, deck, "the deck is the deck on turn {turn}");
    }
}

#[test]
fn a_played_card_leaves_a_hole_and_static_leaves_the_deck() {
    let mut piles = Piles {
        draw: Vec::new(),
        hand: vec![
            Some(Card::Strike),
            Some(Card::Static),
            Some(Card::Wipe),
            Some(Card::Static),
            None,
        ],
        discard: vec![Card::Static],
    };

    assert_eq!(piles.play(4), None, "an empty slot plays nothing");
    assert_eq!(piles.play(9), None, "nor a slot the hand does not have");
    assert_eq!(piles.play(0), Some(Card::Strike));
    assert_eq!(piles.card(0), None);
    assert_eq!(piles.discard, vec![Card::Static, Card::Strike]);

    // Static played is gone; the wipe takes what the hand still holds and
    // leaves the pile's alone.
    assert_eq!(piles.play(1), Some(Card::Static));
    assert_eq!(piles.static_cards(), 2);
    assert_eq!(piles.wipe_hand(), 1);
    assert_eq!(piles.static_cards(), 1);
    assert_eq!(piles.hand, vec![None, None, Some(Card::Wipe), None, None]);

    // Static into the discard pile, up to the cap and no further.
    assert_eq!(piles.add_static(2), 2);
    assert_eq!(piles.add_static(9), STATIC_CAP - 3);
    assert_eq!(piles.add_static(1), 0);
    assert_eq!(piles.static_cards(), STATIC_CAP);
}

/// A draft swaps one card for another: the deck is ten cards whatever
/// was picked, and the drafts come in order, each once its level does.
#[test]
fn a_draft_swaps_one_card_and_the_deck_stays_ten() {
    assert_eq!(deck(&[]), DECK.to_vec());

    let drafted = deck(&[Card::Siphon, Card::Riposte, Card::Burn, Card::Mute]);
    assert_eq!(drafted.len(), DECK.len());
    let held = |card| count(&drafted, card);
    assert_eq!(
        (
            held(Card::Strike),
            held(Card::Block),
            held(Card::Surge),
            held(Card::Wipe)
        ),
        (2, 2, 1, 1),
        "three strikes and a block made way"
    );
    assert_eq!(
        (
            held(Card::Siphon),
            held(Card::Riposte),
            held(Card::Burn),
            held(Card::Mute)
        ),
        (1, 1, 1, 1)
    );
    // Half a build is half the swaps.
    assert_eq!(count(&deck(&[Card::Jab]), Card::Strike), 4);

    assert_eq!(draft_owed(2, 0), None, "nothing before the first level");
    assert_eq!(draft_owed(3, 0), Some(&DRAFTS[0]));
    assert_eq!(draft_owed(3, 1), None, "one draft a level");
    // A runner behind on its picks is owed them in order.
    assert_eq!(draft_owed(12, 1), Some(&DRAFTS[1]));
    assert_eq!(draft_owed(15, DRAFTS.len()), None);
    // Every option costs what the turn can pay, and no card is offered
    // twice.
    let offered: Vec<Card> = DRAFTS.iter().flat_map(|draft| draft.options).collect();
    for (index, card) in offered.iter().enumerate() {
        assert!(card.cost() <= 2, "{card:?}");
        assert!(!offered[..index].contains(card), "{card:?} offered twice");
        assert!(!DECK.contains(card), "{card:?} is a starting card");
    }
}

/// Every card's numbers, from one table: strikes for 10, a surge for 25,
/// blocks of 6, with 4 block standing, two static in the hand, and the
/// glyph at 40 of 100.
#[test]
fn every_card_does_what_it_says() {
    let powers = Powers {
        strike: 10,
        surge: 25,
        block: 6,
        hit: 9,
    };
    let board = Board {
        block: 4,
        foe_signal: 40,
        foe_max_signal: 100,
        statics_in_hand: 2,
    };
    let none = Effect::default();
    let effects: Vec<(Card, Effect)> = [
        Card::Strike,
        Card::Block,
        Card::Surge,
        Card::Wipe,
        Card::Jab,
        Card::Siphon,
        Card::Riposte,
        Card::Bulwark,
        Card::Burn,
        Card::Ground,
        Card::Sever,
        Card::Mute,
        Card::Static,
    ]
    .into_iter()
    .map(|card| (card, card.effect(&powers, &board)))
    .collect();
    assert_eq!(
        effects,
        vec![
            (Card::Strike, Effect { damage: 10, ..none }),
            (Card::Block, Effect { block: 6, ..none }),
            (Card::Surge, Effect { damage: 25, ..none }),
            (
                Card::Wipe,
                Effect {
                    block: 6,
                    clears_hand: true,
                    ..none
                }
            ),
            (Card::Jab, Effect { damage: 5, ..none }),
            (
                Card::Siphon,
                Effect {
                    damage: 10,
                    mend: 5,
                    ..none
                }
            ),
            // A block's worth and the four standing.
            (Card::Riposte, Effect { damage: 10, ..none }),
            (Card::Bulwark, Effect { block: 15, ..none }),
            (
                Card::Burn,
                Effect {
                    energy: 2,
                    static_in: 1,
                    ..none
                }
            ),
            // A strike, and one for each of the two static cards.
            (
                Card::Ground,
                Effect {
                    damage: 30,
                    clears_hand: true,
                    ..none
                }
            ),
            // The glyph is under half: twice the strike.
            (Card::Sever, Effect { damage: 20, ..none }),
            (
                Card::Mute,
                Effect {
                    mutes: true,
                    ..none
                }
            ),
            (Card::Static, none),
        ]
    );
    // Over half, a sever is a strike; at half exactly it is not.
    let over = Board {
        foe_signal: 51,
        ..board
    };
    assert_eq!(Card::Sever.effect(&powers, &over).damage, 10);
    let half = Board {
        foe_signal: 50,
        ..board
    };
    assert_eq!(Card::Sever.effect(&powers, &half).damage, 20);
}
