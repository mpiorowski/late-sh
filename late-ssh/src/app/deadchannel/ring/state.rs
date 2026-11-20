//! The ring's machine (GAME.md, "The arena pass"): two corners, the same
//! flat numbers, a hand each that everybody can see, and [`SLOTS`] slots
//! a round that nobody sees until both are locked.
//!
//! A round: each corner lays up to [`SLOTS`] cards from its hand into
//! slots, in the order they will come out, for [`ENERGY`] energy. When
//! both are locked the slots come out one at a time, both corners' at the
//! same moment: the guards go up, a mute silences the card across from
//! it, then both hits land together. A hit that gets through leaves
//! static in the other deck. A signal at zero ends it, both at zero is a
//! draw, and past the last round the higher signal takes it.
//!
//! Pure: the shuffle's dice are handed in, and the whole bout is one
//! value that lives on its row (`deadchannel_bouts.state`), so any
//! replica can resolve a round and a dropped session finds the same hand.

use late_core::models::deadchannel_bout::Side;
use rand::Rng;
use serde::{Deserialize, Serialize};

use super::data::{self, PURSE_PERCENT, RAKE_PERCENT, ROUNDS, SIGNAL, SLOTS};
use super::policy;
use crate::app::deadchannel::fight::cards::{self, Board, Card, DRAFTS, ENERGY, Effect, Piles};
use crate::app::deadchannel::fight::state::Powers;

/// Both sides, in seat order: the caller sits first.
pub(crate) const SIDES: [Side; 2] = [Side::Caller, Side::Called];

/// Where `side` sits in every pair this file keeps.
fn seat(side: Side) -> usize {
    match side {
        Side::Caller => 0,
        Side::Called => 1,
    }
}

/// One fighter as the bout holds them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Corner {
    pub signal: i32,
    /// Block standing: it holds until a hit eats it, across rounds.
    pub block: i32,
    pub piles: Piles,
    /// The hand slots laid down this round, in the order they come out.
    pub slots: [Option<u8>; SLOTS],
    pub locked: bool,
}

/// What came out of one corner's slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Move {
    /// Nothing was laid there.
    Empty,
    /// The card was laid but the energy for it never came: the burn that
    /// was to pay for it was muted. It stays in the hand.
    Fizzled(Card),
    /// The card across from it was a mute: paid for, and nothing.
    Muted(Card),
    Played(Card),
}

/// One corner's half of a beat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Half {
    pub mv: Move,
    /// Signal this beat took off them.
    pub took: i32,
    /// Their signal and block once the beat was over.
    pub signal: i32,
    pub block: i32,
}

/// One slot resolved for both corners, in seat order.
pub(crate) type Beat = [Half; 2];

/// How a bout ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Ending {
    /// The winner put the other's signal to zero.
    Knockout(Side),
    /// The last round ran out with the winner's signal higher.
    Points(Side),
    /// Both down in the same beat, or level at the end.
    Draw,
}

impl Ending {
    pub(crate) fn winner(self) -> Option<Side> {
        match self {
            Ending::Knockout(side) | Ending::Points(side) => Some(side),
            Ending::Draw => None,
        }
    }
}

/// Why a corner's ask changed nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Refusal {
    /// The bout is over.
    Over,
    /// The corner has locked this round.
    Locked,
    /// No such slot.
    NoSlot,
    /// The hand holds no card there.
    NoCard,
    /// That card is already laid in another slot.
    AlreadyLaid,
    /// The round's energy does not cover the slots as they would stand.
    NoEnergy,
}

/// A bout in progress, or over.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Duel {
    /// The round being laid, from 1.
    pub round: u8,
    /// In seat order.
    corners: [Corner; 2],
    /// Every round resolved so far, beat by beat. A round a knockout cut
    /// short holds fewer than [`SLOTS`] beats.
    pub tape: Vec<Vec<Beat>>,
    pub ending: Option<Ending>,
}

/// What a corner's slot gave up when its turn came.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Drawn {
    Empty,
    Fizzled(Card),
    Card(Card),
}

impl Corner {
    fn deal<R: Rng>(deck: Vec<Card>, rng: &mut R) -> Self {
        Self {
            signal: SIGNAL,
            block: 0,
            piles: Piles::deal(deck, 0, rng),
            slots: [None; SLOTS],
            locked: false,
        }
    }

    /// The card laid in `slot`, paid for out of `energy` and out of the
    /// hand. A static card a wipe already threw out is an empty slot.
    fn draw(&mut self, energy: &mut u8, slot: usize) -> Drawn {
        let Some(hand) = self.slots[slot].map(usize::from) else {
            return Drawn::Empty;
        };
        let Some(card) = self.piles.card(hand) else {
            return Drawn::Empty;
        };
        if card.cost() > *energy {
            return Drawn::Fizzled(card);
        }
        *energy -= card.cost();
        self.piles.play(hand);
        Drawn::Card(card)
    }

    /// The energy the slots leave as they stand, walked in order with
    /// nobody muting them; `None` when a card in them cannot be paid for.
    fn plan(&self, powers: &Powers) -> Option<u8> {
        let mut piles = self.piles.clone();
        let mut energy = ENERGY;
        for hand in self.slots.into_iter().flatten().map(usize::from) {
            let Some(card) = piles.card(hand) else {
                continue;
            };
            if card.cost() > energy {
                return None;
            }
            energy -= card.cost();
            piles.play(hand);
            // Energy and the wipe read nothing off the table but the
            // static in the hand; the rest of the board is not needed.
            let effect = card.effect(
                powers,
                &Board {
                    block: 0,
                    foe_signal: SIGNAL,
                    foe_max_signal: SIGNAL,
                    statics_in_hand: piles.statics_in_hand(),
                },
            );
            energy += effect.energy;
            if effect.clears_hand {
                piles.wipe_hand();
            }
        }
        Some(energy)
    }

    fn laid(&self) -> bool {
        self.slots.iter().any(Option::is_some)
    }

    /// The table as this corner's card reads it, against `foe`.
    pub(crate) fn board(&self, foe: &Corner) -> Board {
        Board {
            block: self.block,
            foe_signal: foe.signal,
            foe_max_signal: SIGNAL,
            statics_in_hand: self.piles.statics_in_hand(),
        }
    }
}

impl Duel {
    /// A bout between two decks: both corners at full signal, the first
    /// hands drawn.
    pub(crate) fn deal<R: Rng>(caller: Vec<Card>, called: Vec<Card>, rng: &mut R) -> Self {
        Self {
            round: 1,
            corners: [Corner::deal(caller, rng), Corner::deal(called, rng)],
            tape: Vec::new(),
            ending: None,
        }
    }

    pub(crate) fn corner(&self, side: Side) -> &Corner {
        &self.corners[seat(side)]
    }

    /// The corner to change: refused once the bout is over or the corner
    /// has locked the round.
    fn open_corner(&mut self, side: Side) -> Result<&mut Corner, Refusal> {
        if self.ending.is_some() {
            return Err(Refusal::Over);
        }
        let corner = &mut self.corners[seat(side)];
        match corner.locked {
            true => Err(Refusal::Locked),
            false => Ok(corner),
        }
    }

    /// Lay the card in hand slot `hand` into `slot`, over whatever was
    /// there. Refused, and nothing moved, when the energy would not cover
    /// the slots as they would then stand.
    pub(crate) fn place(&mut self, side: Side, slot: usize, hand: u8) -> Result<(), Refusal> {
        let powers = data::powers();
        let corner = self.open_corner(side)?;
        if slot >= SLOTS {
            return Err(Refusal::NoSlot);
        }
        if corner.piles.card(usize::from(hand)).is_none() {
            return Err(Refusal::NoCard);
        }
        let elsewhere = corner
            .slots
            .iter()
            .enumerate()
            .any(|(at, laid)| at != slot && *laid == Some(hand));
        if elsewhere {
            return Err(Refusal::AlreadyLaid);
        }
        let before = corner.slots[slot];
        corner.slots[slot] = Some(hand);
        match corner.plan(&powers) {
            Some(_) => Ok(()),
            None => {
                corner.slots[slot] = before;
                Err(Refusal::NoEnergy)
            }
        }
    }

    /// Take back whatever is laid in `slot`. Refused when what stays
    /// leaned on its energy (a burn under a surge).
    pub(crate) fn clear(&mut self, side: Side, slot: usize) -> Result<(), Refusal> {
        let powers = data::powers();
        let corner = self.open_corner(side)?;
        if slot >= SLOTS {
            return Err(Refusal::NoSlot);
        }
        let before = corner.slots[slot].take();
        match corner.plan(&powers) {
            Some(_) => Ok(()),
            None => {
                corner.slots[slot] = before;
                Err(Refusal::NoEnergy)
            }
        }
    }

    /// The round's slots as they stand are the corner's answer. Empty
    /// slots lock too: holding is a move.
    pub(crate) fn lock(&mut self, side: Side) -> Result<(), Refusal> {
        self.open_corner(side)?.locked = true;
        Ok(())
    }

    /// Energy the corner's slots leave this round.
    pub(crate) fn energy_left(&self, side: Side) -> u8 {
        self.corner(side)
            .plan(&data::powers())
            .expect("a corner's slots are always paid for: place and clear refuse the rest")
    }

    /// Both corners locked and the bout still on: the round can resolve.
    pub(crate) fn ready(&self) -> bool {
        self.ending.is_none() && self.corners.iter().all(|corner| corner.locked)
    }

    /// The round's clock ran out: what is laid is locked, and a corner
    /// that laid nothing gets the obvious round laid for it
    /// (`policy::auto`), so a bout with bets on it always finishes.
    pub(crate) fn call_time(&mut self) {
        if self.ending.is_some() {
            return;
        }
        for side in SIDES {
            let corner = &self.corners[seat(side)];
            if corner.locked {
                continue;
            }
            if !corner.laid() {
                let slots = policy::auto(self, side);
                self.corners[seat(side)].slots = slots;
            }
            self.corners[seat(side)].locked = true;
        }
    }

    /// Resolve the round both corners locked: the slots come out in
    /// order, the beats go on the tape, and either the bout ends or the
    /// next round's hands are drawn. The caller holds [`Duel::ready`].
    pub(crate) fn resolve<R: Rng>(&mut self, rng: &mut R) {
        assert!(
            self.ready(),
            "a round resolves once both corners are locked"
        );
        let powers = data::powers();
        let mut energy = [ENERGY; 2];
        let mut beats = Vec::with_capacity(SLOTS);
        for slot in 0..SLOTS {
            beats.push(self.beat(&powers, &mut energy, slot));
            let down = [self.corners[0].signal == 0, self.corners[1].signal == 0];
            self.ending = match down {
                [false, false] => None,
                [true, true] => Some(Ending::Draw),
                [true, false] => Some(Ending::Knockout(Side::Called)),
                [false, true] => Some(Ending::Knockout(Side::Caller)),
            };
            if self.ending.is_some() {
                break;
            }
        }
        self.tape.push(beats);
        if self.ending.is_some() {
            return;
        }
        if self.round >= ROUNDS {
            let [caller, called] = [self.corners[0].signal, self.corners[1].signal];
            self.ending = Some(match caller.cmp(&called) {
                std::cmp::Ordering::Greater => Ending::Points(Side::Caller),
                std::cmp::Ordering::Less => Ending::Points(Side::Called),
                std::cmp::Ordering::Equal => Ending::Draw,
            });
            return;
        }
        self.round += 1;
        for corner in &mut self.corners {
            corner.slots = [None; SLOTS];
            corner.locked = false;
            corner.piles.draw_hand(rng);
        }
    }

    /// One slot, both corners at once.
    fn beat(&mut self, powers: &Powers, energy: &mut [u8; 2], slot: usize) -> Beat {
        let [caller, called] = &mut self.corners;
        let [caller_energy, called_energy] = energy;
        let drawn = [
            caller.draw(caller_energy, slot),
            called.draw(called_energy, slot),
        ];
        // A mute silences the card across from it. Two mutes silence
        // each other.
        let mutes = [
            drawn[0] == Drawn::Card(Card::Mute),
            drawn[1] == Drawn::Card(Card::Mute),
        ];
        let (caller_move, caller_effect) = answer(drawn[0], mutes[1], powers, caller, called);
        let (called_move, called_effect) = answer(drawn[1], mutes[0], powers, called, caller);

        // The guards, the energy and the hand first.
        for (corner, effect, energy) in [
            (&mut *caller, caller_effect, caller_energy),
            (&mut *called, called_effect, called_energy),
        ] {
            corner.block += effect.block;
            *energy += effect.energy;
            if effect.clears_hand {
                corner.piles.wipe_hand();
            }
        }
        // Then both hits, together.
        let called_took = struck(called, caller_effect.damage);
        let caller_took = struck(caller, called_effect.damage);
        // A siphon mends whoever is still standing.
        for (corner, effect) in [(&mut *caller, caller_effect), (&mut *called, called_effect)] {
            if corner.signal > 0 {
                corner.signal = (corner.signal + effect.mend).min(SIGNAL);
            }
        }
        [
            Half {
                mv: caller_move,
                took: caller_took,
                signal: caller.signal,
                block: caller.block,
            },
            Half {
                mv: called_move,
                took: called_took,
                signal: called.signal,
                block: called.block,
            },
        ]
    }

    /// The bout as `viewer` may see it: every hand is open, but the slots
    /// a corner has laid are its own until the round resolves. A watcher
    /// (`None`) sees neither corner's.
    pub(crate) fn view_for(&self, viewer: Option<Side>) -> Duel {
        let mut view = self.clone();
        for side in SIDES {
            if viewer != Some(side) {
                view.corners[seat(side)].slots = [None; SLOTS];
            }
        }
        view
    }
}

/// What a corner's drawn card comes to: the move as the tape tells it,
/// and what it does. A muted or fizzled card does nothing.
fn answer(
    drawn: Drawn,
    muted: bool,
    powers: &Powers,
    own: &Corner,
    other: &Corner,
) -> (Move, Effect) {
    match (drawn, muted) {
        (Drawn::Empty, _) => (Move::Empty, Effect::default()),
        (Drawn::Fizzled(card), _) => (Move::Fizzled(card), Effect::default()),
        (Drawn::Card(card), true) => (Move::Muted(card), Effect::default()),
        (Drawn::Card(card), false) => (Move::Played(card), card.effect(powers, &own.board(other))),
    }
}

/// A hit of `incoming` on `corner`: the block eats what it can, the rest
/// comes off the signal, and a hit that lands at all leaves static in
/// the deck. Returns what landed.
fn struck(corner: &mut Corner, incoming: i32) -> i32 {
    let held = corner.block.min(incoming);
    corner.block -= held;
    let landed = incoming - held;
    if landed > 0 {
        corner.signal = (corner.signal - landed).max(0);
        corner.piles.add_static(1);
    }
    landed
}

/// The draft picks a runner brings to the ring: one option of every
/// draft, in draft order. Not the road's drafts: the ring asks the four
/// questions again, of everyone, whatever their level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Picks([Card; DRAFTS.len()]);

impl Picks {
    /// What a runner who never chose brings: the first option of each.
    pub(crate) fn opening() -> Self {
        Self(DRAFTS.map(|draft| draft.options[0]))
    }

    /// The picks a row holds. `None` for anything that is not one option
    /// of every draft, in order.
    pub(crate) fn parse(value: &serde_json::Value) -> Option<Self> {
        let cards: [Card; DRAFTS.len()] = serde_json::from_value(value.clone()).ok()?;
        DRAFTS
            .iter()
            .zip(&cards)
            .all(|(draft, card)| draft.options.contains(card))
            .then_some(Self(cards))
    }

    pub(crate) fn to_json(self) -> serde_json::Value {
        serde_json::json!(self.0)
    }

    pub(crate) fn cards(&self) -> &[Card; DRAFTS.len()] {
        &self.0
    }

    /// These picks with draft `index` flipped to its other option.
    pub(crate) fn flipped(mut self, index: usize) -> Self {
        self.0[index] = DRAFTS[index].other(self.0[index]);
        self
    }

    /// The deck these picks make for a runner who inked the drafts at
    /// `inks`: ink is the one thing off the road that the ring keeps.
    pub(crate) fn deck(&self, inks: &[i32]) -> Vec<Card> {
        cards::deck(&self.0, inks)
    }
}

/// The bits bet on each corner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Pool {
    pub caller: i64,
    pub called: i64,
}

impl Pool {
    pub(crate) fn on(self, side: Side) -> i64 {
        match side {
            Side::Caller => self.caller,
            Side::Called => self.called,
        }
    }

    pub(crate) fn total(self) -> i64 {
        self.caller + self.called
    }
}

/// What a finished bout does with its pool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Settlement {
    /// Every stake goes back whole: a draw, or nobody on one of the
    /// corners, so there was nothing to win.
    Refund,
    /// The rake is burned, `purse` goes to the winning fighter, and
    /// `split` is shared among the bets on `winner` by their size.
    Paid {
        winner: Side,
        purse: i64,
        split: i64,
    },
}

/// How `pool` settles on `ending`. No bits are made: the purse and the
/// split together are the pool less the rake.
pub(crate) fn settle(pool: Pool, ending: Ending) -> Settlement {
    let Some(winner) = ending.winner() else {
        return Settlement::Refund;
    };
    if pool.caller == 0 || pool.called == 0 {
        return Settlement::Refund;
    }
    let total = pool.total();
    let rake = total * RAKE_PERCENT / 100;
    let purse = total * PURSE_PERCENT / 100;
    Settlement::Paid {
        winner,
        purse,
        split: total - rake - purse,
    }
}

/// What a bet of `bits` on `side` is paid, rounded down: the dust left by
/// the division is burned with the rake.
pub(crate) fn pay(pool: Pool, settlement: Settlement, side: Side, bits: i64) -> i64 {
    match settlement {
        Settlement::Refund => bits,
        Settlement::Paid { winner, split, .. } => match side == winner {
            true => split * bits / pool.on(winner),
            false => 0,
        },
    }
}

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;
