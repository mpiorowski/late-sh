//! The standard 52-card deck the rummy-family games on the roster share.
//! Cribbage and gin rummy agree on everything here: ace low, a run is
//! consecutive ranks in one suit, and a card's count value is its pip with
//! every face card worth ten. Briscola is not one of them (40 cards, its own
//! rank order), so it keeps its own card type.
//!
//! A card serializes as its id (`suit * 13 + rank`), so a deal is a compact
//! array of numbers in the state JSON and deserializing rejects anything out
//! of range: an illegal card can't exist, and the rules never re-check for
//! one.

use rand::Rng;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::app::games::cards::{CardRank, CardSuit, PlayingCard};

pub const DECK: usize = 52;

/// Suit order is the card id's high part, so this array is load-bearing.
pub const SUITS: [CardSuit; 4] = [
    CardSuit::Hearts,
    CardSuit::Diamonds,
    CardSuit::Clubs,
    CardSuit::Spades,
];

/// Ace low: declaration order is run order, and doubles as the id's low part.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Rank {
    Ace,
    Two,
    Three,
    Four,
    Five,
    Six,
    Seven,
    Eight,
    Nine,
    Ten,
    Jack,
    Queen,
    King,
}

impl Rank {
    pub const ALL: [Self; 13] = [
        Self::Ace,
        Self::Two,
        Self::Three,
        Self::Four,
        Self::Five,
        Self::Six,
        Self::Seven,
        Self::Eight,
        Self::Nine,
        Self::Ten,
        Self::Jack,
        Self::Queen,
        Self::King,
    ];

    /// 1 for the ace up to 13 for the king: runs are consecutive numbers.
    pub const fn number(self) -> u8 {
        match self {
            Self::Ace => 1,
            Self::Two => 2,
            Self::Three => 3,
            Self::Four => 4,
            Self::Five => 5,
            Self::Six => 6,
            Self::Seven => 7,
            Self::Eight => 8,
            Self::Nine => 9,
            Self::Ten => 10,
            Self::Jack => 11,
            Self::Queen => 12,
            Self::King => 13,
        }
    }

    /// The count value: pips, with every face card worth ten. Cribbage adds
    /// these to fifteen and thirty-one; gin rummy counts deadwood in them.
    pub const fn value(self) -> u32 {
        match self {
            Self::Ace => 1,
            Self::Two => 2,
            Self::Three => 3,
            Self::Four => 4,
            Self::Five => 5,
            Self::Six => 6,
            Self::Seven => 7,
            Self::Eight => 8,
            Self::Nine => 9,
            Self::Ten | Self::Jack | Self::Queen | Self::King => 10,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Ace => "A",
            Self::Two => "2",
            Self::Three => "3",
            Self::Four => "4",
            Self::Five => "5",
            Self::Six => "6",
            Self::Seven => "7",
            Self::Eight => "8",
            Self::Nine => "9",
            Self::Ten => "10",
            Self::Jack => "J",
            Self::Queen => "Q",
            Self::King => "K",
        }
    }

    /// The shared card renderer's rank, for drawing a face.
    pub const fn card_rank(self) -> CardRank {
        match self {
            Self::Ace => CardRank::Ace,
            Self::Jack => CardRank::Jack,
            Self::Queen => CardRank::Queen,
            Self::King => CardRank::King,
            Self::Two
            | Self::Three
            | Self::Four
            | Self::Five
            | Self::Six
            | Self::Seven
            | Self::Eight
            | Self::Nine
            | Self::Ten => CardRank::Number(self.number()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Card {
    pub suit: CardSuit,
    pub rank: Rank,
}

impl Card {
    pub const fn new(rank: Rank, suit: CardSuit) -> Self {
        Self { suit, rank }
    }

    pub const fn id(self) -> u8 {
        suit_order(self.suit) * 13 + (self.rank.number() - 1)
    }

    pub fn from_id(id: u8) -> Option<Self> {
        let suit = *SUITS.get((id / 13) as usize)?;
        let rank = *Rank::ALL.get((id % 13) as usize)?;
        Some(Self { suit, rank })
    }

    pub const fn value(self) -> u32 {
        self.rank.value()
    }

    /// `10♥`: the move-feed and history spelling.
    pub fn label(self) -> String {
        format!("{}{}", self.rank.label(), self.suit.symbol())
    }

    /// The shared card renderer's card, for drawing a face.
    pub const fn playing_card(self) -> PlayingCard {
        PlayingCard {
            suit: self.suit,
            rank: self.rank.card_rank(),
        }
    }
}

pub const fn suit_order(suit: CardSuit) -> u8 {
    match suit {
        CardSuit::Hearts => 0,
        CardSuit::Diamonds => 1,
        CardSuit::Clubs => 2,
        CardSuit::Spades => 3,
    }
}

impl Serialize for Card {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_u8(self.id())
    }
}

impl<'de> Deserialize<'de> for Card {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let id = u8::deserialize(deserializer)?;
        Self::from_id(id)
            .ok_or_else(|| serde::de::Error::custom(format!("card id out of range: {id}")))
    }
}

pub fn fresh_deck() -> Vec<Card> {
    SUITS
        .into_iter()
        .flat_map(|suit| Rank::ALL.into_iter().map(move |rank| Card { suit, rank }))
        .collect()
}

/// A Fisher-Yates shuffle of a fresh deck. Called by the service only, when
/// a hand is dealt; every draw after it replays this order.
pub fn shuffled_deck(rng: &mut impl Rng) -> Vec<Card> {
    let mut deck = fresh_deck();
    for index in (1..deck.len()).rev() {
        deck.swap(index, rng.gen_range(0..=index));
    }
    deck
}

/// A deal must be the whole deck, each card once: the rules index into it
/// and take hands as set differences, so this is the one structural check
/// worth making at the boundary.
pub fn is_whole_deck(deal: &[Card]) -> bool {
    if deal.len() != DECK {
        return false;
    }
    let mut seen = [false; DECK];
    for card in deal {
        let slot = &mut seen[card.id() as usize];
        if *slot {
            return false;
        }
        *slot = true;
    }
    true
}

/// Low to high by rank, suits in id order within a rank: the order a hand
/// reads in when nothing better applies.
pub fn sort_by_rank(cards: &mut [Card]) {
    cards.sort_by_key(|card| (card.rank, suit_order(card.suit)));
}

#[cfg(test)]
#[path = "std_deck_test.rs"]
mod std_deck_test;
