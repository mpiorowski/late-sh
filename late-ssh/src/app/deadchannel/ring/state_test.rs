use late_core::models::deadchannel_bout::Side;
use rand::{SeedableRng, rngs::StdRng};

use super::{Beat, Corner, Duel, Ending, Move, Pool, Refusal, Settlement, pay, settle};
use crate::app::deadchannel::fight::cards::{Card, DECK, Piles, deck};
use crate::app::deadchannel::ring::data::{ROUNDS, SIGNAL, SLOTS, powers};

/// A corner holding exactly `hand`, nothing in the draw pile.
fn corner(hand: [Card; 5]) -> Corner {
    Corner {
        signal: SIGNAL,
        block: 0,
        piles: Piles {
            draw: Vec::new(),
            hand: hand.map(Some).to_vec(),
            discard: Vec::new(),
        },
        slots: [None; SLOTS],
        locked: false,
    }
}

fn duel(caller: [Card; 5], called: [Card; 5]) -> Duel {
    Duel {
        round: 1,
        corners: [corner(caller), corner(called)],
        tape: Vec::new(),
        ending: None,
    }
}

/// Lay `(slot, hand)` pairs for both corners, lock, resolve, and hand
/// back the round's beats.
fn round(duel: &mut Duel, caller: &[(usize, u8)], called: &[(usize, u8)]) -> Vec<Beat> {
    for (side, laid) in [(Side::Caller, caller), (Side::Called, called)] {
        for (slot, hand) in laid {
            duel.place(side, *slot, *hand).expect("the slot is laid");
        }
        duel.lock(side).expect("the corner locks");
    }
    duel.resolve(&mut StdRng::seed_from_u64(7));
    duel.tape
        .last()
        .expect("a resolved round is on the tape")
        .clone()
}

const FIVE_STRIKES: [Card; 5] = [Card::Strike; 5];

/// What a corner has laid, as `(slot, hand)` pairs, the empty slots left
/// out.
fn laid(duel: &Duel, side: Side) -> Vec<(usize, u8)> {
    duel.corner(side)
        .slots
        .iter()
        .enumerate()
        .filter_map(|(slot, hand)| hand.map(|hand| (slot, hand)))
        .collect()
}

#[test]
fn a_block_laid_in_the_same_slot_eats_the_hit_and_what_lands_leaves_static() {
    let p = powers();
    let mut duel = duel(
        [
            Card::Surge,
            Card::Strike,
            Card::Strike,
            Card::Strike,
            Card::Strike,
        ],
        [
            Card::Block,
            Card::Strike,
            Card::Strike,
            Card::Strike,
            Card::Strike,
        ],
    );

    let beats = round(&mut duel, &[(0, 0)], &[(0, 0)]);

    let [caller, called] = beats[0];
    assert_eq!(
        (caller.mv, called.mv, called.took, called.block, caller.took),
        (
            Move::Played(Card::Surge),
            Move::Played(Card::Block),
            p.surge - p.block,
            0,
            0
        )
    );
    // The tape keeps what was thrown as well as what landed: the
    // difference is what the block ate.
    assert_eq!((caller.hit, called.hit), (p.surge, 0));
    assert_eq!(
        duel.corner(Side::Called).signal,
        SIGNAL - (p.surge - p.block)
    );
    assert_eq!(
        (
            duel.corner(Side::Called).piles.static_cards(),
            duel.corner(Side::Caller).piles.static_cards()
        ),
        (1, 0)
    );
}

#[test]
fn a_jab_eats_the_block_and_the_surge_behind_it_lands_whole() {
    let p = powers();
    let mut duel = duel(
        [
            Card::Jab,
            Card::Surge,
            Card::Strike,
            Card::Strike,
            Card::Strike,
        ],
        [
            Card::Block,
            Card::Strike,
            Card::Strike,
            Card::Strike,
            Card::Strike,
        ],
    );

    let beats = round(&mut duel, &[(0, 0), (1, 1)], &[(0, 0)]);

    assert_eq!(
        (beats[0][1].took, beats[1][1].took),
        ((p.jab - p.block).max(0), p.surge)
    );
}

#[test]
fn a_mute_silences_the_card_across_from_it_and_no_other() {
    let p = powers();
    let mut duel = duel(
        [
            Card::Strike,
            Card::Surge,
            Card::Strike,
            Card::Strike,
            Card::Strike,
        ],
        [
            Card::Mute,
            Card::Strike,
            Card::Strike,
            Card::Strike,
            Card::Strike,
        ],
    );

    // The caller strikes, then surges; the mute sits across from the surge.
    let beats = round(&mut duel, &[(0, 0), (1, 1)], &[(1, 0)]);

    assert_eq!(
        (beats[0][0].mv, beats[0][1].took),
        (Move::Played(Card::Strike), p.strike)
    );
    assert_eq!(
        (beats[1][0].mv, beats[1][1].mv, beats[1][1].took),
        (Move::Muted(Card::Surge), Move::Played(Card::Mute), 0)
    );
}

#[test]
fn a_muted_burn_leaves_the_card_it_was_paying_for_in_the_hand() {
    let mut duel = duel(
        [
            Card::Burn,
            Card::Surge,
            Card::Surge,
            Card::Strike,
            Card::Strike,
        ],
        [
            Card::Mute,
            Card::Strike,
            Card::Strike,
            Card::Strike,
            Card::Strike,
        ],
    );

    let beats = round(&mut duel, &[(0, 0), (1, 1), (2, 2)], &[(0, 0)]);

    assert_eq!(
        (beats[0][0].mv, beats[1][0].mv, beats[2][0].mv),
        (
            Move::Muted(Card::Burn),
            Move::Played(Card::Surge),
            Move::Fizzled(Card::Surge)
        )
    );
}

#[test]
fn a_riposte_hits_for_itself_and_the_block_standing_and_the_block_stays() {
    let p = powers();
    let mut duel = duel(
        [
            Card::Block,
            Card::Riposte,
            Card::Strike,
            Card::Strike,
            Card::Strike,
        ],
        FIVE_STRIKES,
    );

    let beats = round(&mut duel, &[(0, 0), (1, 1)], &[]);

    assert_eq!(
        (beats[1][1].took, beats[1][0].block),
        (p.riposte + p.block, p.block)
    );
}

#[test]
fn a_sever_is_a_strike_until_the_other_corner_is_at_half() {
    let p = powers();
    let hand = [
        Card::Sever,
        Card::Strike,
        Card::Strike,
        Card::Strike,
        Card::Strike,
    ];
    let mut fresh = duel(hand, FIVE_STRIKES);
    let mut halved = duel(hand, FIVE_STRIKES);
    halved.corners[1].signal = SIGNAL / 2;

    let fresh_beats = round(&mut fresh, &[(0, 0)], &[]);
    let halved_beats = round(&mut halved, &[(0, 0)], &[]);

    assert_eq!(
        (fresh_beats[0][1].took, halved_beats[0][1].took),
        (p.strike, p.sever)
    );
}

#[test]
fn both_corners_down_in_one_beat_is_a_draw_and_the_round_stops_there() {
    let mut duel = duel(FIVE_STRIKES, FIVE_STRIKES);
    duel.corners[0].signal = 1;
    duel.corners[1].signal = 1;

    let beats = round(&mut duel, &[(0, 0), (1, 1)], &[(0, 0), (1, 1)]);

    assert_eq!((duel.ending, beats.len()), (Some(Ending::Draw), 1));
    assert_eq!(duel.place(Side::Caller, 0, 2), Err(Refusal::Over));
}

#[test]
fn a_knockout_names_the_corner_still_standing() {
    let mut duel = duel(FIVE_STRIKES, FIVE_STRIKES);
    duel.corners[1].signal = 1;

    round(&mut duel, &[(0, 0)], &[(2, 0)]);

    assert_eq!(duel.ending, Some(Ending::Knockout(Side::Caller)));
}

#[test]
fn past_the_last_round_the_higher_signal_takes_it_and_level_is_a_draw() {
    let mut ahead = duel(FIVE_STRIKES, FIVE_STRIKES);
    ahead.round = ROUNDS;
    let mut level = ahead.clone();

    round(&mut ahead, &[], &[(0, 0)]);
    round(&mut level, &[(0, 0)], &[(0, 0)]);

    assert_eq!(
        (ahead.ending, level.ending),
        (Some(Ending::Points(Side::Called)), Some(Ending::Draw))
    );
}

#[test]
fn a_slot_is_refused_when_the_energy_does_not_cover_it_or_the_card_is_laid() {
    let mut duel = duel(
        [
            Card::Burn,
            Card::Surge,
            Card::Surge,
            Card::Strike,
            Card::Strike,
        ],
        FIVE_STRIKES,
    );

    // Two surges are four energy: only a burn ahead of them pays.
    assert_eq!(duel.place(Side::Caller, 1, 1), Ok(()));
    assert_eq!(duel.place(Side::Caller, 2, 2), Err(Refusal::NoEnergy));
    assert_eq!(duel.place(Side::Caller, 0, 0), Ok(()));
    assert_eq!(duel.place(Side::Caller, 2, 2), Ok(()));
    assert_eq!(duel.energy_left(Side::Caller), 1);
    // The burn cannot come back out from under them, and no card is
    // laid twice.
    assert_eq!(duel.clear(Side::Caller, 0), Err(Refusal::NoEnergy));
    assert_eq!(duel.place(Side::Caller, 0, 1), Err(Refusal::AlreadyLaid));
    assert_eq!(laid(&duel, Side::Caller), [(0, 0), (1, 1), (2, 2)]);

    duel.lock(Side::Caller).expect("the corner locks");
    assert_eq!(duel.clear(Side::Caller, 2), Err(Refusal::Locked));
}

#[test]
fn nobody_sees_a_corner_s_slots_but_the_corner() {
    let mut duel = duel(FIVE_STRIKES, FIVE_STRIKES);
    duel.place(Side::Caller, 0, 3).expect("laid");
    duel.place(Side::Called, 1, 4).expect("laid");
    duel.lock(Side::Caller).expect("locked");

    let slots = |viewer| {
        let view = duel.view_for(viewer);
        (laid(&view, Side::Caller), laid(&view, Side::Called))
    };
    assert_eq!(slots(None), (vec![], vec![]));
    assert_eq!(slots(Some(Side::Caller)), (vec![(0, 3)], vec![]));
    assert_eq!(slots(Some(Side::Called)), (vec![], vec![(1, 4)]));
    // Whether a corner has locked is no secret.
    assert!(duel.view_for(None).corner(Side::Caller).locked);
}

#[test]
fn the_clock_locks_what_is_laid_and_lays_a_round_for_a_corner_that_laid_nothing() {
    let mut duel = duel(FIVE_STRIKES, FIVE_STRIKES);
    duel.place(Side::Caller, 2, 4).expect("laid");

    duel.call_time();

    assert!(duel.ready());
    assert_eq!(laid(&duel, Side::Caller), [(2, 4)]);
    assert_eq!(laid(&duel, Side::Called), [(0, 0), (1, 1), (2, 2)]);
}

#[test]
fn a_pool_pays_the_winning_side_by_stake_and_makes_no_bits() {
    let pool = Pool {
        caller: 300,
        called: 100,
    };
    let settlement = settle(pool, Ending::Knockout(Side::Called));

    assert_eq!(
        settlement,
        Settlement::Paid {
            winner: Side::Called,
            purse: 40,
            split: 320
        }
    );
    // Two bets of 75 and 25 on the winner, three of 100 on the loser.
    let paid = [
        pay(pool, settlement, Side::Called, 75),
        pay(pool, settlement, Side::Called, 25),
        pay(pool, settlement, Side::Caller, 100),
    ];
    assert_eq!(paid, [240, 80, 0]);
    assert!(paid.iter().sum::<i64>() + 40 <= pool.total());
}

#[test]
fn a_draw_or_a_one_sided_pool_hands_every_stake_back() {
    let both = Pool {
        caller: 300,
        called: 100,
    };
    let one_sided = Pool {
        caller: 300,
        called: 0,
    };

    let drawn = settle(both, Ending::Draw);
    let unopposed = settle(one_sided, Ending::Points(Side::Caller));

    assert_eq!((drawn, unopposed), (Settlement::Refund, Settlement::Refund));
    assert_eq!(pay(both, drawn, Side::Caller, 120), 120);
    assert_eq!(pay(one_sided, unopposed, Side::Caller, 300), 300);
}

/// The tape as one line a round: each beat `caller/called`, a card by its
/// name, `-` for an empty slot, `~` ahead of a muted card and `!` ahead
/// of a fizzled one.
fn tape(duel: &Duel) -> Vec<String> {
    let word = |mv: Move| match mv {
        Move::Empty => "-".to_string(),
        Move::Played(card) => card.name().to_string(),
        Move::Muted(card) => format!("~{}", card.name()),
        Move::Fizzled(card) => format!("!{}", card.name()),
    };
    duel.tape
        .iter()
        .map(|beats| {
            beats
                .iter()
                .map(|[caller, called]| format!("{}/{}", word(caller.mv), word(called.mv)))
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect()
}

#[test]
fn a_seeded_bout_on_the_clock_plays_out_the_same_every_time() {
    let mut rng = StdRng::seed_from_u64(11);
    let drafted = deck(&[Card::Jab, Card::Riposte, Card::Ground, Card::Mute], &[]);
    let mut duel = Duel::deal(DECK.to_vec(), drafted, &mut rng);

    while duel.ending.is_none() {
        duel.call_time();
        duel.resolve(&mut rng);
    }

    assert_eq!(
        (
            duel.ending,
            duel.round,
            duel.corner(Side::Caller).signal,
            duel.corner(Side::Called).signal,
            duel.corner(Side::Caller).block,
            duel.corner(Side::Called).block,
            duel.corner(Side::Caller).piles.static_cards(),
            duel.corner(Side::Called).piles.static_cards(),
            tape(&duel),
        ),
        (
            // Both corners went down in the same beat of the third round.
            Some(Ending::Draw),
            3,
            0,
            0,
            0,
            0,
            5,
            5,
            vec![
                "strike/strike siphon/wipe wipe/block -/-".to_string(),
                "surge/jab strike/riposte -/surge -/-".to_string(),
                "strike/jab".to_string(),
            ],
        )
    );
}
