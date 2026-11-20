//! The round the ring lays for a corner that laid nothing before the
//! clock ran out (`Duel::call_time`). It is the obvious round and no
//! more, and it is the floor: a runner who shows up and reads the other
//! hand beats it, the way a hand played well beats the road's `Auto` key.
//!
//! Two steps over the open hands, both pure:
//!
//! - [`obvious`]: a burn first, for the energy; a guard if the other hand
//!   holds something heavy and nothing is standing, the biggest it has;
//!   then every hit the
//!   energy covers, the free ones first and the best rate after; then
//!   whatever guard or static card the leftovers pay for.
//! - [`auto`]: the obvious round, except that a mute in the hand goes
//!   into the slot where the other corner's obvious round puts its
//!   heaviest hit. It reads the other corner as if they were the ring's
//!   own hand, so it is right about a runner who went quiet and wrong
//!   about one who did not.
//!
//! Every card is laid through `Duel::place`, so the machine and never
//! this file decides what the energy covers.

use late_core::models::deadchannel_bout::Side;

use super::data::{self, SLOTS};
use super::state::Duel;
use crate::app::deadchannel::fight::cards::{Card, Effect};
use crate::app::deadchannel::fight::state::Powers;

/// A hit worth a mute, or a guard put up ahead of it: two strikes or
/// more in one card.
fn heavy(powers: &Powers, damage: i32) -> bool {
    damage >= powers.strike * 2
}

/// What `card` would do from `side`'s hand as the table stands.
fn effect_of(duel: &Duel, side: Side, powers: &Powers, card: Card) -> Effect {
    card.effect(powers, &duel.corner(side).board(duel.corner(side.other())))
}

/// The hand as `(hand slot, card)`, in hand order.
fn hand_of(duel: &Duel, side: Side) -> Vec<(u8, Card)> {
    duel.corner(side)
        .piles
        .hand
        .iter()
        .enumerate()
        .filter_map(|(at, card)| card.map(|card| (at as u8, card)))
        .collect()
}

/// The order the obvious round tries the hand in. `Duel::place` cuts it
/// to what the energy covers.
fn order(duel: &Duel, side: Side, powers: &Powers) -> Vec<u8> {
    let hand = hand_of(duel, side);
    let other = side.other();
    let threatened = duel.corner(side).block == 0
        && hand_of(duel, other)
            .iter()
            .any(|(_, card)| heavy(powers, effect_of(duel, other, powers, *card).damage));

    let mut burns = Vec::new();
    let mut guards = Vec::new();
    let mut hits = Vec::new();
    let mut statics = Vec::new();
    for (at, card) in hand {
        let effect = effect_of(duel, side, powers, card);
        match card {
            Card::Burn => burns.push(at),
            Card::Static => statics.push(at),
            // A mute is never laid blind: `auto` aims it or leaves it.
            Card::Mute => {}
            Card::Block | Card::Wipe | Card::Brace | Card::Bulwark => {
                guards.push((card.cost(), effect.block, at));
            }
            Card::Strike
            | Card::Surge
            | Card::Jab
            | Card::Siphon
            | Card::Riposte
            | Card::Ground
            | Card::Sever => hits.push((card.cost(), effect.damage, at)),
        }
    }
    // The cheapest guard first; the free hits first, then the best
    // damage for the energy (compared as cross products, no division).
    guards.sort();
    hits.sort_by(|a, b| {
        let (a_cost, a_damage, a_at) = *a;
        let (b_cost, b_damage, b_at) = *b;
        let a_rate = a_damage * i32::from(b_cost.max(1));
        let b_rate = b_damage * i32::from(a_cost.max(1));
        (a_cost > 0)
            .cmp(&(b_cost > 0))
            .then(b_rate.cmp(&a_rate))
            .then(a_at.cmp(&b_at))
    });

    let mut order = burns;
    // Against something heavy, the guard that holds the most; the rest
    // wait for the leftovers.
    let wall = guards
        .iter()
        .max_by_key(|(cost, block, _)| (*block, std::cmp::Reverse(*cost)))
        .map(|(_, _, at)| *at)
        .filter(|_| threatened);
    order.extend(wall);
    order.extend(hits.into_iter().map(|(_, _, at)| at));
    order.extend(
        guards
            .into_iter()
            .map(|(_, _, at)| at)
            .filter(|at| Some(*at) != wall),
    );
    order.extend(statics);
    order
}

/// Lay `order` into the free slots of `side`'s corner on `trial`, first
/// slot first, skipping what the machine refuses.
fn lay(trial: &mut Duel, side: Side, order: &[u8]) {
    for hand in order {
        let Some(slot) = trial.corner(side).slots.iter().position(Option::is_none) else {
            return;
        };
        // A refusal is a card the energy does not cover: the next one
        // in the order may still fit.
        let _ = trial.place(side, slot, *hand);
    }
}

/// The obvious round for `side`, as slots.
pub(crate) fn obvious(duel: &Duel, side: Side) -> [Option<u8>; SLOTS] {
    let powers = data::powers();
    let mut trial = duel.clone();
    lay(&mut trial, side, &order(duel, side, &powers));
    trial.corner(side).slots
}

/// The round laid for a corner that laid nothing: the obvious one, with
/// a mute aimed at the other corner's heaviest obvious hit when the hand
/// holds one and there is a hit there worth it.
pub(crate) fn auto(duel: &Duel, side: Side) -> [Option<u8>; SLOTS] {
    let powers = data::powers();
    let other = side.other();
    let mute = hand_of(duel, side)
        .into_iter()
        .find(|(_, card)| *card == Card::Mute)
        .map(|(at, _)| at);
    let theirs = obvious(duel, other);
    let heaviest = theirs
        .iter()
        .enumerate()
        .filter_map(|(slot, hand)| {
            let card = duel.corner(other).piles.card(usize::from((*hand)?))?;
            let damage = effect_of(duel, other, &powers, card).damage;
            heavy(&powers, damage).then_some((damage, slot))
        })
        .max_by_key(|(damage, slot)| (*damage, std::cmp::Reverse(*slot)))
        .map(|(_, slot)| slot);

    let mut trial = duel.clone();
    if let (Some(mute), Some(slot)) = (mute, heaviest) {
        // Refused only when the energy cannot cover a mute at all.
        let _ = trial.place(side, slot, mute);
    }
    lay(&mut trial, side, &order(duel, side, &powers));
    trial.corner(side).slots
}

#[cfg(test)]
#[path = "policy_test.rs"]
mod policy_test;
