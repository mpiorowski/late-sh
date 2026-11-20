use late_core::models::deadchannel_bout::Side;
use rand::{SeedableRng, rngs::StdRng};

use super::{auto, obvious};
use crate::app::deadchannel::fight::cards::{Card, DRAFTS, deck};
use crate::app::deadchannel::ring::data::SLOTS;
use crate::app::deadchannel::ring::state::{Duel, Ending};

/// A bout whose first hands are exactly these: each deck is the hand and
/// nothing else, so the deal draws all of it.
fn duel(caller: [Card; 5], called: [Card; 5]) -> Duel {
    Duel::deal(
        caller.to_vec(),
        called.to_vec(),
        &mut StdRng::seed_from_u64(3),
    )
}

/// The cards `slots` lay, each with the slot it comes out of, the empty
/// slots left out.
fn cards(duel: &Duel, side: Side, slots: [Option<u8>; SLOTS]) -> Vec<(usize, Card)> {
    slots
        .into_iter()
        .enumerate()
        .filter_map(|(slot, hand)| {
            let card = duel.corner(side).piles.card(usize::from(hand?))?;
            Some((slot, card))
        })
        .collect()
}

const QUIET: [Card; 5] = [Card::Block; 5];

#[test]
fn the_obvious_round_is_the_free_hit_then_the_best_the_energy_covers() {
    let duel = duel(
        [
            Card::Strike,
            Card::Surge,
            Card::Jab,
            Card::Block,
            Card::Strike,
        ],
        QUIET,
    );

    let slots = obvious(&duel, Side::Caller);

    assert_eq!(
        cards(&duel, Side::Caller, slots),
        [(0, Card::Jab), (1, Card::Surge), (2, Card::Strike)]
    );
}

#[test]
fn a_corner_with_nothing_up_guards_first_against_a_heavy_hand() {
    let duel = duel(
        [
            Card::Strike,
            Card::Strike,
            Card::Block,
            Card::Strike,
            Card::Strike,
        ],
        [
            Card::Surge,
            Card::Block,
            Card::Block,
            Card::Block,
            Card::Block,
        ],
    );

    let slots = obvious(&duel, Side::Caller);

    assert_eq!(
        cards(&duel, Side::Caller, slots),
        [(0, Card::Block), (1, Card::Strike), (2, Card::Strike)]
    );
}

#[test]
fn a_mute_goes_across_from_the_other_corner_s_obvious_heavy_hit() {
    let duel = duel(
        [
            Card::Mute,
            Card::Strike,
            Card::Strike,
            Card::Strike,
            Card::Strike,
        ],
        [
            Card::Jab,
            Card::Surge,
            Card::Strike,
            Card::Block,
            Card::Block,
        ],
    );

    // The other corner's obvious round is jab, surge, strike.
    let slots = auto(&duel, Side::Caller);

    assert_eq!(
        cards(&duel, Side::Caller, slots),
        [(0, Card::Strike), (1, Card::Mute)]
    );
}

#[test]
fn the_mute_is_aimed_by_the_open_hand_and_never_by_what_the_other_corner_laid() {
    let mut duel = duel(
        [
            Card::Mute,
            Card::Strike,
            Card::Strike,
            Card::Strike,
            Card::Strike,
        ],
        [
            Card::Jab,
            Card::Surge,
            Card::Strike,
            Card::Block,
            Card::Block,
        ],
    );
    let open_hand = auto(&duel, Side::Caller);

    // The other corner hides its surge in the first slot and locks: the
    // clock's hand lays the round it would have laid without knowing.
    let surge = (0..5u8)
        .find(|at| duel.corner(Side::Called).piles.card(usize::from(*at)) == Some(Card::Surge))
        .expect("a surge in the hand");
    duel.place(Side::Called, 0, surge)
        .expect("the surge is laid");
    duel.lock(Side::Called).expect("locked");

    assert_eq!(auto(&duel, Side::Caller), open_hand);
    assert_eq!(
        cards(&duel, Side::Caller, open_hand),
        [(0, Card::Strike), (1, Card::Mute)]
    );
}

/// Every deck the four drafts make, in draft order.
fn decks() -> Vec<Vec<Card>> {
    (0..16)
        .map(|picks: usize| {
            let drafted: Vec<Card> = DRAFTS
                .iter()
                .enumerate()
                .map(|(at, draft)| draft.options[(picks >> at) & 1])
                .collect();
            deck(&drafted, &[])
        })
        .collect()
}

/// One bout played to the end on the clock alone.
fn play(caller: &[Card], called: &[Card], seed: u64) -> (Ending, u8) {
    let mut rng = StdRng::seed_from_u64(seed);
    let mut duel = Duel::deal(caller.to_vec(), called.to_vec(), &mut rng);
    loop {
        duel.call_time();
        duel.resolve(&mut rng);
        if let Some(ending) = duel.ending {
            return (ending, duel.round);
        }
    }
}

/// The ring's balance, as one contract: every deck against every deck,
/// from both seats, on the clock's own hand. No deck wins fewer than one
/// bout in three or more than two in three, and most bouts end in a
/// knockout before the points do. The clock's hand is the floor and it
/// plays a mute and a bulwark worse than a person does, so the decks
/// built on them sit at the low end here; what a read hand makes of them
/// is not measured. A number in `data.rs` moves until this passes; the
/// band moves only with a design decision.
#[test]
fn no_deck_runs_away_with_the_ring_on_the_clock() {
    const BOUTS: u64 = 60;
    let decks = decks();
    let mut knockouts = 0u64;
    let mut bouts = 0u64;
    let mut rounds = 0u64;
    let mut report = Vec::new();
    let mut worst = (50u64, 50u64);
    for (mine, deck) in decks.iter().enumerate() {
        let (mut won, mut lost) = (0u64, 0u64);
        for (theirs, other) in decks.iter().enumerate() {
            if mine == theirs {
                continue;
            }
            for seed in 0..BOUTS {
                for ((ending, round), me) in [
                    (play(deck, other, seed), Side::Caller),
                    (play(other, deck, seed), Side::Called),
                ] {
                    bouts += 1;
                    rounds += u64::from(round);
                    if matches!(ending, Ending::Knockout(_)) {
                        knockouts += 1;
                    }
                    match ending.winner() {
                        Some(side) if side == me => won += 1,
                        Some(_) => lost += 1,
                        None => {}
                    }
                }
            }
        }
        let percent = won * 100 / (won + lost);
        worst = (worst.0.min(percent), worst.1.max(percent));
        report.push(format!(
            "deck {mine:02}: {percent}% ({won} won, {lost} lost)"
        ));
    }
    let knockout_percent = knockouts * 100 / bouts;
    let report = format!(
        "{}\nknockouts: {knockout_percent}% of {bouts} bouts, {}.{} rounds a bout",
        report.join("\n"),
        rounds / bouts,
        rounds * 10 / bouts % 10
    );
    println!("{report}");
    assert!(
        worst.0 >= 33 && worst.1 <= 67,
        "a deck is outside one third to two thirds:\n{report}"
    );
    assert!(
        knockout_percent >= 50,
        "fewer than half the bouts end in a knockout:\n{report}"
    );
}
