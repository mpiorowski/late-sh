//! The deck and the hand (GAME.md, "The round: ten cards, three energy").
//! Everyone starts with the same ten cards; the weapon and the armor are
//! what the cards hit and hold for, so the armorer's wall is still the
//! power curve and the deck is a spreadsheet with two columns.
//!
//! The deck never grows. At the levels in [`DRAFTS`] a runner is owed a
//! new card: one of two, the same two for everybody at that level, and
//! the one taken goes in over a strike or a block ([`deck`]). Four drafts,
//! sixteen decks, every one of them small enough for the arena to fight
//! (`fight/BALANCE.md`). Every new card's number is one of the four the
//! basic cards already print ([`Card::effect`]), so the wall still prices
//! all of it.
//!
//! Static is the one card nobody chose: a hit that lands puts one in the
//! discard pile, and it rides the deck for the rest of the day's road.
//! It costs one to play and does nothing but leave. One metaphor for the
//! signal, the face, and the hand.
//!
//! Pure: the shuffle's dice are handed in, and the piles are on the row
//! (`Fight::piles`) so a dropped session finds the same hand waiting and a
//! reconnect never deals a better one.

use rand::Rng;
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};

use super::state::Powers;

/// Cards drawn at the top of every turn.
pub const HAND: usize = 5;
/// Energy at the top of every turn.
pub const ENERGY: u8 = 3;
/// The most static a deck holds: a hand's worth. Past it a hit still
/// hurts, it just has nowhere left to put noise.
pub const STATIC_CAP: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Card {
    /// Hits for the strike.
    Strike,
    /// Holds the block until a hit eats it.
    Block,
    /// Two strikes and half of a third, for two energy.
    Surge,
    /// A block, and every static card in the hand gone for good.
    Wipe,
    /// Half a strike, for nothing.
    Jab,
    /// A strike that mends you for half of itself.
    Siphon,
    /// Hits for a block's worth and for every point of block standing;
    /// the block stays.
    Riposte,
    /// Two blocks and half of a third, for two energy.
    Bulwark,
    /// Two energy now, and a static card into your deck.
    Burn,
    /// A strike, and one more for every static card in the hand, which
    /// goes with it for good.
    Ground,
    /// A strike, twice over once the glyph is at half its signal or less.
    Sever,
    /// Whatever the glyph meant to do this turn does nothing.
    Mute,
    /// Does nothing. Played, it leaves the deck.
    Static,
}

/// One draft: at `level` a runner picks one of `options`, and it takes
/// the place of one `replaces` in the deck.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Draft {
    pub level: i32,
    pub replaces: Card,
    pub options: [Card; 2],
}

/// The drafts, in the order they are owed. Each is a question with no
/// right answer: tempo or sustain, armor as a weapon or as a wall, static
/// as fuel or as ammunition, the finisher or the cut-out.
pub const DRAFTS: [Draft; 4] = [
    Draft {
        level: 3,
        replaces: Card::Strike,
        options: [Card::Jab, Card::Siphon],
    },
    Draft {
        level: 6,
        replaces: Card::Block,
        options: [Card::Riposte, Card::Bulwark],
    },
    Draft {
        level: 9,
        replaces: Card::Strike,
        options: [Card::Burn, Card::Ground],
    },
    Draft {
        level: 12,
        replaces: Card::Strike,
        options: [Card::Sever, Card::Mute],
    },
];

/// The draft a runner of `level` holding `drafted` cards is owed: the
/// next one in [`DRAFTS`], once the level reaches it.
pub fn draft_owed(level: i32, drafted: usize) -> Option<&'static Draft> {
    DRAFTS.get(drafted).filter(|draft| level >= draft.level)
}

/// The deck of a runner who drafted `drafted`, in order: [`DECK`] with
/// each pick over one of the cards its draft replaces. The caller holds
/// that `drafted[i]` is an option of `DRAFTS[i]` (`Sheet::from_row` checks
/// the row, `Command::Draft` the pick).
pub fn deck(drafted: &[Card]) -> Vec<Card> {
    let mut deck = DECK.to_vec();
    for (draft, pick) in DRAFTS.iter().zip(drafted) {
        let slot = deck
            .iter()
            .position(|card| *card == draft.replaces)
            .expect("the deck holds a card for every draft to replace");
        deck[slot] = *pick;
    }
    deck
}

/// What a card's numbers read off the table besides the powers: the
/// fight as it stands when the card is played.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Board {
    /// Block standing.
    pub block: i32,
    pub foe_signal: i32,
    pub foe_max_signal: i32,
    /// Static cards in the hand.
    pub statics_in_hand: usize,
}

/// What playing one card does. The one place the cards' numbers are
/// worked out: the machine applies it (`Sheet::play`), the policies
/// weigh it (`policy.rs`), the hand prints it (`ui.rs`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Effect {
    /// Off the glyph's signal.
    pub damage: i32,
    /// Onto the block standing.
    pub block: i32,
    /// Onto the runner's signal, up to its max.
    pub mend: i32,
    /// Energy handed back this turn.
    pub energy: u8,
    /// Static cards into the deck.
    pub static_in: usize,
    /// Every static card in the hand leaves the deck.
    pub clears_hand: bool,
    /// The glyph's move this turn does nothing.
    pub mutes: bool,
}

/// Half of `n`, rounded up: a jab's share of a strike, a siphon's mend.
fn half(n: i32) -> i32 {
    (n + 1) / 2
}

/// Two and a half of `n`, the half rounded up: the surge's share of a
/// strike (`Rules::surge` is this), the bulwark's of a block.
fn two_and_a_half(n: i32) -> i32 {
    n * 2 + half(n)
}

/// The deck every runner carries, before the day puts static in it.
pub const DECK: [Card; 10] = [
    Card::Strike,
    Card::Strike,
    Card::Strike,
    Card::Strike,
    Card::Strike,
    Card::Block,
    Card::Block,
    Card::Block,
    Card::Surge,
    Card::Wipe,
];

impl Card {
    pub fn cost(self) -> u8 {
        match self {
            Card::Jab | Card::Burn => 0,
            Card::Strike
            | Card::Block
            | Card::Wipe
            | Card::Siphon
            | Card::Riposte
            | Card::Ground
            | Card::Sever
            | Card::Static => 1,
            Card::Surge | Card::Bulwark | Card::Mute => 2,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Card::Strike => "strike",
            Card::Block => "block",
            Card::Surge => "surge",
            Card::Wipe => "wipe",
            Card::Jab => "jab",
            Card::Siphon => "siphon",
            Card::Riposte => "riposte",
            Card::Bulwark => "bulwark",
            Card::Burn => "burn",
            Card::Ground => "ground",
            Card::Sever => "sever",
            Card::Mute => "mute",
            Card::Static => "static",
        }
    }

    /// What the card does, in a line short enough for the draft's offer
    /// on the road panel (`ui.rs`); the guide prints it too.
    pub fn rule(self) -> &'static str {
        match self {
            Card::Strike => "hits for your strike",
            Card::Block => "holds your block until a hit eats it",
            Card::Surge => "two energy. hits for two strikes and a half",
            Card::Wipe => "a block, and every static card in your hand thrown out",
            Card::Jab => "free. hits for half a strike",
            Card::Siphon => "a strike that mends you for half of what it hits",
            Card::Riposte => "hits for a block's worth plus all the block you have up",
            Card::Bulwark => "two energy. holds two blocks and a half",
            Card::Burn => "free. two more energy now, one static card in your deck",
            Card::Ground => "a strike, plus one per static card in hand. they go too",
            Card::Sever => "a strike. twice as hard once the glyph is at half or less",
            Card::Mute => "two energy. the glyph's move this turn does nothing",
            Card::Static => "a dead card. play it to throw it out for good",
        }
    }

    /// What playing the card does, with `powers` on a table that stands
    /// as `board`.
    pub fn effect(self, powers: &Powers, board: &Board) -> Effect {
        let none = Effect::default();
        match self {
            Card::Strike => Effect {
                damage: powers.strike,
                ..none
            },
            Card::Surge => Effect {
                damage: powers.surge,
                ..none
            },
            Card::Block => Effect {
                block: powers.block,
                ..none
            },
            Card::Wipe => Effect {
                block: powers.block,
                clears_hand: true,
                ..none
            },
            Card::Jab => Effect {
                damage: half(powers.strike),
                ..none
            },
            Card::Siphon => Effect {
                damage: powers.strike,
                mend: half(powers.strike),
                ..none
            },
            Card::Riposte => Effect {
                damage: powers.block + board.block,
                ..none
            },
            Card::Bulwark => Effect {
                block: two_and_a_half(powers.block),
                ..none
            },
            Card::Burn => Effect {
                energy: BURN_ENERGY,
                static_in: 1,
                ..none
            },
            Card::Ground => Effect {
                damage: powers.strike * (1 + board.statics_in_hand as i32),
                clears_hand: true,
                ..none
            },
            Card::Sever => Effect {
                damage: match board.foe_signal * 2 <= board.foe_max_signal {
                    true => powers.strike * 2,
                    false => powers.strike,
                },
                ..none
            },
            Card::Mute => Effect {
                mutes: true,
                ..none
            },
            Card::Static => none,
        }
    }
}

/// Energy a burn hands back.
pub const BURN_ENERGY: u8 = 2;

/// The three piles of a fight in progress. The hand keeps its slots: a
/// played card leaves a hole, so the number on a card is the number you
/// press for the whole turn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Piles {
    /// Drawn from the end.
    pub draw: Vec<Card>,
    pub hand: Vec<Option<Card>>,
    pub discard: Vec<Card>,
}

impl Piles {
    /// `deck` with `static_cards` of the day's static in it, shuffled, and
    /// the first hand drawn.
    pub fn deal<R: Rng>(deck: Vec<Card>, static_cards: usize, rng: &mut R) -> Self {
        let mut draw = deck;
        draw.extend(std::iter::repeat_n(
            Card::Static,
            static_cards.min(STATIC_CAP),
        ));
        draw.shuffle(rng);
        let mut piles = Self {
            draw,
            hand: Vec::new(),
            discard: Vec::new(),
        };
        piles.draw_hand(rng);
        piles
    }

    /// What is left of the hand goes to the discard pile and a fresh hand
    /// comes off the draw pile, the discard shuffled back under it when it
    /// runs dry. A deck shorter than a hand deals what it has.
    pub fn draw_hand<R: Rng>(&mut self, rng: &mut R) {
        self.discard.extend(self.hand.drain(..).flatten());
        for _ in 0..HAND {
            if self.draw.is_empty() {
                self.draw.append(&mut self.discard);
                self.draw.shuffle(rng);
            }
            let card = self.draw.pop();
            self.hand.push(card);
        }
    }

    /// The card in `slot`, if the slot holds one.
    pub fn card(&self, slot: usize) -> Option<Card> {
        self.hand.get(slot).copied().flatten()
    }

    /// Play the card in `slot`: it leaves the hand for the discard pile,
    /// or, for static, leaves the deck.
    pub fn play(&mut self, slot: usize) -> Option<Card> {
        let card = self.hand.get_mut(slot)?.take()?;
        match card {
            Card::Static => {}
            Card::Strike
            | Card::Block
            | Card::Surge
            | Card::Wipe
            | Card::Jab
            | Card::Siphon
            | Card::Riposte
            | Card::Bulwark
            | Card::Burn
            | Card::Ground
            | Card::Sever
            | Card::Mute => self.discard.push(card),
        }
        Some(card)
    }

    /// Every static card in the hand out of the deck. Returns how many.
    pub fn wipe_hand(&mut self) -> usize {
        let mut wiped = 0;
        for slot in &mut self.hand {
            if *slot == Some(Card::Static) {
                *slot = None;
                wiped += 1;
            }
        }
        wiped
    }

    /// Static in the hand.
    pub fn statics_in_hand(&self) -> usize {
        self.hand
            .iter()
            .filter(|slot| **slot == Some(Card::Static))
            .count()
    }

    /// Static in the deck, wherever it lies.
    pub fn static_cards(&self) -> usize {
        let is_static = |card: &&Card| **card == Card::Static;
        self.draw.iter().filter(is_static).count()
            + self.discard.iter().filter(is_static).count()
            + self.hand.iter().flatten().filter(is_static).count()
    }

    /// Up to `cards` of static into the discard pile, stopping at
    /// [`STATIC_CAP`]. Returns how many went in.
    pub fn add_static(&mut self, cards: usize) -> usize {
        let room = STATIC_CAP.saturating_sub(self.static_cards());
        let added = cards.min(room);
        self.discard
            .extend(std::iter::repeat_n(Card::Static, added));
        added
    }
}

#[cfg(test)]
#[path = "cards_test.rs"]
mod cards_test;
