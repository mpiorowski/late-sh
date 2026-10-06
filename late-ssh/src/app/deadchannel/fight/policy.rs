//! How a turn gets played without a person choosing the cards. Two
//! policies over one read of the table, both pure, both playing the cards
//! through the one place their numbers live (`Card::effect`):
//!
//! - [`auto`] is the game's own `Auto` key (GAME.md, "The round": the
//!   daily floor for the ambient runner). It is the obvious turn and no
//!   more: finish the glyph if the hand can; on the turn a heavy lands,
//!   mute it or put two guards up; hit with everything else, the best
//!   rate first; and spend what is left shaking static out. It never
//!   blocks a plain hit and never thinks a turn ahead: the block that
//!   could have gone up while the glyph was gathering is the hand's to
//!   find.
//! - [`sharp`] is a runner who reads the hand: every order of every
//!   affordable set of cards is played out and weighed, signal against
//!   signal. It lives here for the sim and the arena, where the gap
//!   between the two is the number that says what playing the cards is
//!   worth (`fight/BALANCE.md`). The game never plays it for anybody.
//!
//! The road's threat word is the fight played out on `auto`
//! (`sim::odds`): it is the floor, and a hand played well beats the word.

use super::cards::{Board, Card, Effect, STATIC_CAP};
use super::data::{Intent, NOISE_CARDS, Rules};
use super::state::{Fight, Powers, Sheet};

/// Everything a policy reads to play one turn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Table {
    pub hand: Vec<Option<Card>>,
    pub energy: u8,
    /// Block already standing.
    pub block: i32,
    pub foe_signal: i32,
    pub foe_max_signal: i32,
    /// The runner's signal, and its max.
    pub signal: i32,
    pub max_signal: i32,
    /// Static the deck still has room for.
    pub static_room: usize,
    pub powers: Powers,
    /// What the glyph does at the end of this turn, and of the next.
    pub now: Intent,
    pub next: Intent,
}

impl Table {
    pub fn read(sheet: &Sheet, rules: &Rules, fight: &Fight) -> Self {
        Self {
            hand: fight.piles.hand.clone(),
            energy: fight.energy,
            block: fight.block,
            foe_signal: fight.foe_signal,
            foe_max_signal: fight.foe_max_signal,
            signal: sheet.signal,
            max_signal: sheet.max_signal(),
            static_room: STATIC_CAP.saturating_sub(fight.piles.static_cards()),
            powers: sheet.powers_under(rules, fight),
            now: fight.intent(),
            next: fight.intent_in(1),
        }
    }

    /// What lands on the runner at the end of a turn showing `intent`,
    /// before block.
    fn incoming(&self, intent: Intent) -> i32 {
        match intent {
            Intent::Hit => self.powers.hit,
            Intent::Heavy => self.powers.hit * 2,
            Intent::Charge | Intent::Noise => 0,
        }
    }
}

/// A turn as a policy plays it out in its head: the table, a few cards
/// in.
#[derive(Debug, Clone)]
struct Turn {
    hand: Vec<Option<Card>>,
    energy: u8,
    block: i32,
    /// Dealt to the glyph so far.
    damage: i32,
    /// Mended so far, before the signal's max caps it.
    mend: i32,
    /// Static out of the deck, and into it.
    cleared: usize,
    added: usize,
    muted: bool,
    /// Energy paid.
    spent: u8,
    /// The slots played, in order.
    plays: Vec<usize>,
}

impl Turn {
    fn open(table: &Table) -> Self {
        Self {
            hand: table.hand.clone(),
            energy: table.energy,
            block: table.block,
            damage: 0,
            mend: 0,
            cleared: 0,
            added: 0,
            muted: false,
            spent: 0,
            plays: Vec::new(),
        }
    }

    fn slots_of(&self, card: Card) -> Vec<usize> {
        self.hand
            .iter()
            .enumerate()
            .filter(|(_, held)| **held == Some(card))
            .map(|(slot, _)| slot)
            .collect()
    }

    fn statics_in_hand(&self) -> usize {
        self.slots_of(Card::Static).len()
    }

    /// The card in `slot` and what it would do now, when the slot holds
    /// one the turn can pay for.
    fn reads(&self, table: &Table, slot: usize) -> Option<(Card, Effect)> {
        let card = self.hand.get(slot).copied().flatten()?;
        if card.cost() > self.energy {
            return None;
        }
        let board = Board {
            block: self.block,
            foe_signal: (table.foe_signal - self.damage).max(0),
            foe_max_signal: table.foe_max_signal,
            statics_in_hand: self.statics_in_hand(),
        };
        Some((card, card.effect(&table.powers, &board)))
    }

    /// Play `slot`, as the machine would. False, and nothing moved, when
    /// the slot is empty or the turn cannot pay.
    fn play(&mut self, table: &Table, slot: usize) -> bool {
        let Some((card, effect)) = self.reads(table, slot) else {
            return false;
        };
        self.hand[slot] = None;
        self.energy = self.energy - card.cost() + effect.energy;
        self.spent += card.cost();
        self.damage += effect.damage;
        self.block += effect.block;
        self.mend += effect.mend;
        self.muted = self.muted || effect.mutes;
        if card == Card::Static {
            self.cleared += 1;
        }
        if effect.clears_hand {
            for held in &mut self.hand {
                if *held == Some(Card::Static) {
                    *held = None;
                    self.cleared += 1;
                }
            }
        }
        let room = (table.static_room + self.cleared).saturating_sub(self.added);
        self.added += effect.static_in.min(room);
        self.plays.push(slot);
        true
    }

    /// Hit with everything the energy covers, the best damage for the
    /// energy first: a free card ahead of a paid one, and of two as good
    /// the one that is not a sever, which only gets better as the glyph
    /// gets lower.
    fn hit(&mut self, table: &Table) {
        loop {
            let best = (0..self.hand.len())
                .filter_map(|slot| {
                    let (card, effect) = self.reads(table, slot)?;
                    match effect.damage > 0 {
                        true => Some((slot, card, effect.damage)),
                        false => None,
                    }
                })
                .max_by(|a, b| {
                    // Damage for the energy, cross-multiplied; a free
                    // card costs less than any paid one.
                    let rate = |(_, card, damage): &(usize, Card, i32)| {
                        (i64::from(*damage), i64::from(card.cost()))
                    };
                    let ((da, ca), (db, cb)) = (rate(a), rate(b));
                    let by_rate = match (ca, cb) {
                        (0, 0) => da.cmp(&db),
                        (0, _) => std::cmp::Ordering::Greater,
                        (_, 0) => std::cmp::Ordering::Less,
                        (ca, cb) => (da * cb).cmp(&(db * ca)),
                    };
                    by_rate
                        .then((a.1 != Card::Sever).cmp(&(b.1 != Card::Sever)))
                        // The first of equals: the lower slot.
                        .then(b.0.cmp(&a.0))
                });
            let Some((slot, _, _)) = best else {
                return;
            };
            self.play(table, slot);
        }
    }

    /// A burn, when the hand holds one and has more to play than the
    /// energy covers.
    fn burn(&mut self, table: &Table) {
        let Some(slot) = self.slots_of(Card::Burn).into_iter().next() else {
            return;
        };
        let wanted: u8 = self
            .hand
            .iter()
            .flatten()
            .filter(|card| **card != Card::Burn)
            .map(|card| card.cost())
            .sum();
        if wanted > self.energy {
            self.play(table, slot);
        }
    }
}

/// The most cards the `Auto` key spends on block in one turn.
const AUTO_GUARDS: usize = 2;

/// The obvious turn: the slots to play, in order, before ending it.
pub fn auto(table: &Table) -> Vec<usize> {
    // The kill, when the hand holds it.
    let mut all_out = Turn::open(table);
    all_out.burn(table);
    all_out.hit(table);
    if all_out.damage >= table.foe_signal {
        return all_out.plays;
    }
    let mut turn = Turn::open(table);
    turn.burn(table);
    // Guard only on the turn a heavy lands: a mute when the hand holds
    // one, else never more than [`AUTO_GUARDS`] cards of block. The rest
    // of the turn still hits.
    let heavy = match table.now {
        Intent::Heavy => table.powers.hit * 2,
        Intent::Hit | Intent::Charge | Intent::Noise => 0,
    };
    let mute = turn.slots_of(Card::Mute).into_iter().next();
    let muted = match (turn.block < heavy, mute) {
        (true, Some(slot)) => turn.play(table, slot),
        (true, None) | (false, _) => false,
    };
    if !muted {
        // The wipe ahead of the blocks while there is static to wipe,
        // behind them when there is none; the bulwark ahead of both.
        let wipes = turn.slots_of(Card::Wipe);
        let blocks = turn.slots_of(Card::Block);
        let mut guards = turn.slots_of(Card::Bulwark);
        match turn.statics_in_hand() > 0 {
            true => guards.extend(wipes.iter().chain(&blocks)),
            false => guards.extend(blocks.iter().chain(&wipes)),
        }
        let mut up = 0;
        for slot in guards {
            if turn.block >= heavy || up == AUTO_GUARDS {
                break;
            }
            if turn.play(table, slot) {
                up += 1;
            }
        }
    }
    turn.hit(table);
    // What is left shakes static out.
    for slot in turn.slots_of(Card::Static) {
        turn.play(table, slot);
    }
    turn.plays
}

/// What one static card in the deck costs, in points of signal, as a
/// share of the glyph's hit: a dead draw is about a third of a turn's
/// card.
const STATIC_WEIGHT: f64 = 0.35;
/// What block still standing after the glyph's turn is worth against the
/// next one, a little under par: the fight may end first.
const BANKED_WEIGHT: f64 = 0.85;
/// Damage a full turn of cards deals, in strikes: what a point of the
/// glyph's signal is worth in turns of being hit.
const STRIKES_A_TURN: f64 = 2.5;

/// What a turn played this far is worth, were it ended here.
fn score(turn: &Turn, table: &Table) -> f64 {
    if turn.damage >= table.foe_signal {
        // The kill, the cheapest way to it, and no static bought for it.
        return 1.0e6 - f64::from(turn.spent) - turn.added as f64;
    }
    let hit = f64::from(table.powers.hit);
    let per_point = hit / (STRIKES_A_TURN * f64::from(table.powers.strike.max(1)));
    let incoming = match turn.muted {
        true => 0,
        false => table.incoming(table.now),
    };
    let taken = (incoming - turn.block).max(0);
    let left = (turn.block - incoming).max(0);
    let noise = match (turn.muted, table.now, taken > 0) {
        (true, _, _) => 0.0,
        (false, Intent::Noise, _) => NOISE_CARDS as f64,
        (false, Intent::Hit | Intent::Heavy | Intent::Charge, true) => 1.0,
        (false, Intent::Hit | Intent::Heavy | Intent::Charge, false) => 0.0,
    };
    let mended = turn.mend.min(table.max_signal - table.signal).max(0);
    let banked = left.min(table.incoming(table.next));
    let dead = match taken >= table.signal + mended {
        true => -1.0e9,
        false => 0.0,
    };
    dead + f64::from(turn.damage) * per_point - f64::from(taken)
        + f64::from(mended)
        + (turn.cleared as f64 - turn.added as f64 - noise) * STATIC_WEIGHT * hit
        + f64::from(banked) * BANKED_WEIGHT
}

/// Every turn that starts with `turn`, played out a card at a time; the
/// best one found is left in `best`.
fn search(turn: &Turn, table: &Table, best: &mut (f64, Vec<usize>)) {
    for slot in 0..turn.hand.len() {
        let mut next = turn.clone();
        if !next.play(table, slot) {
            continue;
        }
        let worth = score(&next, table);
        if worth > best.0 {
            *best = (worth, next.plays.clone());
        }
        if next.damage < table.foe_signal {
            search(&next, table, best);
        }
    }
}

/// A turn read properly: every order of every affordable set of cards
/// weighed, the best played. The slots, in order, before ending the turn.
/// Of two turns as good, the one found first: the lower slots.
pub fn sharp(table: &Table) -> Vec<usize> {
    let open = Turn::open(table);
    let mut best = (score(&open, table), Vec::new());
    search(&open, table, &mut best);
    best.1
}

#[cfg(test)]
#[path = "policy_test.rs"]
mod policy_test;
