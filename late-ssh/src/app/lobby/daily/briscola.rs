//! Briscola rules for daily correspondence matches. Pure state + logic, no
//! I/O: the service persists `DailyBriscolaState` as the match's `state`
//! JSON the same way chess and battleship persist theirs.
//!
//! The state stores the deal and the play history, nothing else. Hands, the
//! stock, whose turn it is, and both scores are derived by replaying that
//! history, so the state can never self-contradict. The whole shuffled deck
//! is decided at claim time, which means there is no mid-match randomness to
//! make durable: contrast backgammon, whose dice are server-rolled per turn
//! and stored.
//!
//! Hidden information is a RENDERING rule here, not a data one. This state
//! holds both hands and the stock, exactly like battleship holds both
//! fleets. The TUI renders server-side over SSH, so nothing in the state
//! reaches a player the renderer does not draw; `briscola_ui` draws only the
//! viewer's own hand and shows everything else as backs.
//!
//! v1 rules: 40 cards (a standard deck with the 8s, 9s, and 10s removed),
//! three dealt to each player, the next card turned face up as the trump and
//! left under the stock so it is the last card drawn. The leader plays any
//! card and the follower answers with any card: there is no obligation to
//! follow suit. The highest trump takes the trick, else the highest card of
//! the led suit; the winner draws first and leads the next trick. 120 points
//! are on the table, so passing 60 settles it, and the match ends the moment
//! either player is past 60. An even 60-60 split after all twenty tricks is
//! a draw.
//!
//! One deliberate deviation from the table game: both captured-point totals
//! are shown throughout. Traditional briscola lets you count only your own
//! pile, because remembering the rest is part of the skill. Across a match
//! played a move a day that stops being memory and starts being note-taking,
//! so the board shows both.

use anyhow::{Context, Result, ensure};
use rand::Rng;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use uuid::Uuid;

use crate::app::games::cards::{CardRank, CardSuit, PlayingCard};

/// A standard deck minus the 8s, 9s, and 10s.
pub const DECK: usize = 40;
/// Cards held at a time, until the stock runs dry near the end.
pub const HAND: usize = 3;
/// Deck layout: two opening hands, the turned-up trump, then the stock in
/// draw order. The trump sits at its own index because it is turned up at
/// the deal but drawn last.
const TRUMP_INDEX: usize = HAND * 2;
const STOCK_START: usize = TRUMP_INDEX + 1;
/// Every card that reaches a hand after the deal, the trump included.
pub const DRAWS: usize = DECK - HAND * 2;
/// Points on the table across the whole deck.
pub const TOTAL_POINTS: u32 = 120;
/// More than half of `TOTAL_POINTS`: reaching it settles the match.
pub const WINNING_POINTS: u32 = 61;
const STATE_VERSION: u8 = 1;

/// Suit order is the card id's high digit, so this array is load-bearing.
const SUITS: [CardSuit; 4] = [
    CardSuit::Hearts,
    CardSuit::Diamonds,
    CardSuit::Clubs,
    CardSuit::Spades,
];

/// The ten briscola ranks. Declaration order is trick strength, strongest
/// first, and doubles as the card id's low digit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rank {
    Ace,
    Three,
    King,
    Queen,
    Jack,
    Seven,
    Six,
    Five,
    Four,
    Two,
}

impl Rank {
    pub const ALL: [Self; 10] = [
        Self::Ace,
        Self::Three,
        Self::King,
        Self::Queen,
        Self::Jack,
        Self::Seven,
        Self::Six,
        Self::Five,
        Self::Four,
        Self::Two,
    ];

    /// Card points. The ace and the three carry the deck: 21 of the 120 sit
    /// in a single suit's top two cards.
    pub const fn points(self) -> u32 {
        match self {
            Self::Ace => 11,
            Self::Three => 10,
            Self::King => 4,
            Self::Queen => 3,
            Self::Jack => 2,
            Self::Seven | Self::Six | Self::Five | Self::Four | Self::Two => 0,
        }
    }

    /// Position in `ALL`: lower is stronger.
    const fn order(self) -> u8 {
        match self {
            Self::Ace => 0,
            Self::Three => 1,
            Self::King => 2,
            Self::Queen => 3,
            Self::Jack => 4,
            Self::Seven => 5,
            Self::Six => 6,
            Self::Five => 7,
            Self::Four => 8,
            Self::Two => 9,
        }
    }

    /// Within one suit, does this rank take the trick from `other`?
    pub const fn beats(self, other: Self) -> bool {
        self.order() < other.order()
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Ace => "A",
            Self::Three => "3",
            Self::King => "K",
            Self::Queen => "Q",
            Self::Jack => "J",
            Self::Seven => "7",
            Self::Six => "6",
            Self::Five => "5",
            Self::Four => "4",
            Self::Two => "2",
        }
    }

    /// The shared card renderer's rank, for drawing a face.
    pub const fn card_rank(self) -> CardRank {
        match self {
            Self::Ace => CardRank::Ace,
            Self::Three => CardRank::Number(3),
            Self::King => CardRank::King,
            Self::Queen => CardRank::Queen,
            Self::Jack => CardRank::Jack,
            Self::Seven => CardRank::Number(7),
            Self::Six => CardRank::Number(6),
            Self::Five => CardRank::Number(5),
            Self::Four => CardRank::Number(4),
            Self::Two => CardRank::Number(2),
        }
    }
}

/// One of the forty. Serialized as its id so the deal is a compact array of
/// numbers in the state JSON, and deserializing rejects anything out of
/// range: an illegal card can't exist, so the rules never re-check for one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Card {
    pub suit: CardSuit,
    pub rank: Rank,
}

impl Card {
    /// `suit * 10 + rank`, both by declaration order.
    pub const fn id(self) -> u8 {
        suit_order(self.suit) * 10 + self.rank.order()
    }

    pub fn from_id(id: u8) -> Option<Self> {
        let suit = *SUITS.get((id / 10) as usize)?;
        let rank = *Rank::ALL.get((id % 10) as usize)?;
        Some(Self { suit, rank })
    }

    pub const fn points(self) -> u32 {
        self.rank.points()
    }

    /// `A♥`: the move-feed and history spelling.
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

const fn suit_order(suit: CardSuit) -> u8 {
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

/// Everything the deal plus the play history imply. Rebuilt on demand; the
/// renderer must read only the viewer's own entry in `hands`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Table {
    /// Cards still held, per seat.
    pub hands: [Vec<Card>; 2],
    /// Points captured, per seat.
    pub points: [u32; 2],
    /// Tricks captured, per seat.
    pub tricks: [usize; 2],
    /// The seat to play next.
    pub turn: usize,
    /// The card already on the table this trick, and who led it.
    pub led: Option<(usize, Card)>,
    /// Cards gone from the stock, the trump included.
    pub drawn: usize,
    /// Completed tricks in play order, for the history rail.
    pub history: Vec<TrickRecord>,
    /// The most recent card played, and by whom.
    pub last: Option<(usize, Card)>,
}

/// One finished trick: who led what, who answered, and what it was worth.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TrickRecord {
    pub leader: usize,
    pub lead: Card,
    pub answer: Card,
    pub winner: usize,
    pub points: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TrickWin {
    pub winner: usize,
    /// Points in the two cards just taken.
    pub points: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatchEnd {
    Winner(usize),
    Draw,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayOutcome {
    pub seat: usize,
    pub card: Card,
    /// Set when this card closed a trick.
    pub trick: Option<TrickWin>,
    /// Captured points per seat after the play.
    pub totals: [u32; 2],
    pub finish: Option<MatchEnd>,
}

impl PlayOutcome {
    /// `A♥` / `A♥, takes 21` / `A♥, loses 21, 64-56`: the move-feed label,
    /// spoken from the mover's side, so a card that closes a trick says
    /// whether the mover took it or gave it away.
    pub fn label(&self) -> String {
        let card = self.card.label();
        let Some(trick) = self.trick else {
            return card;
        };
        let taken = match (trick.winner == self.seat, trick.points) {
            (true, 0) => format!("{card}, takes the trick"),
            (true, points) => format!("{card}, takes {points}"),
            (false, 0) => format!("{card}, loses the trick"),
            (false, points) => format!("{card}, loses {points}"),
        };
        match self.finish {
            None => taken,
            Some(MatchEnd::Draw) => format!("{taken}, drawn 60-60"),
            Some(MatchEnd::Winner(seat)) => {
                format!(
                    "{taken}, {}-{}",
                    self.totals[seat],
                    self.totals[other(seat)]
                )
            }
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DailyBriscolaState {
    pub version: u8,
    #[serde(default)]
    pub revision: u64,
    /// Seat 0 leads the first trick; the claim-time coin flip picks who sits
    /// there.
    pub seats: [Uuid; 2],
    /// The whole deal, fixed at claim time: three cards each, the trump,
    /// then the stock in draw order.
    pub deck: Vec<Card>,
    /// Cards in play order, two per trick.
    pub plays: Vec<Card>,
}

impl DailyBriscolaState {
    pub fn new(challenger: Uuid, claimer: Uuid) -> Self {
        let mut rng = rand::thread_rng();
        let seats = if rng.gen_bool(0.5) {
            [challenger, claimer]
        } else {
            [claimer, challenger]
        };
        let mut deck = fresh_deck();
        // Fisher-Yates. This is the whole match's randomness, spent here so
        // every draw afterwards is a replay of a fixed order.
        for index in (1..deck.len()).rev() {
            deck.swap(index, rng.gen_range(0..=index));
        }
        Self {
            version: STATE_VERSION,
            revision: 0,
            seats,
            deck,
            plays: Vec::new(),
        }
    }

    pub fn parse(value: &Value) -> Result<Self> {
        let state: Self =
            serde_json::from_value(value.clone()).context("corrupt daily match state")?;
        ensure!(
            state.version == STATE_VERSION,
            "unsupported daily briscola state version: {}",
            state.version
        );
        // The deal must be a permutation of the deck: everything downstream
        // indexes into it and takes hands as a set difference, so this is
        // the one structural check worth making at the boundary.
        ensure!(
            state.deck.len() == DECK,
            "daily briscola deal is not a deck"
        );
        let mut seen = [false; DECK];
        for card in &state.deck {
            let slot = &mut seen[card.id() as usize];
            ensure!(!*slot, "daily briscola deal repeats a card");
            *slot = true;
        }
        ensure!(
            state.plays.len() <= DECK,
            "daily briscola has more plays than cards"
        );
        Ok(state)
    }

    /// The turned-up trump. Its suit outranks every other suit; the card
    /// itself is the last one drawn.
    pub fn trump(&self) -> Card {
        self.deck[TRUMP_INDEX]
    }

    pub fn seat_of(&self, user_id: Uuid) -> Option<usize> {
        self.seats.iter().position(|&seat| seat == user_id)
    }

    pub fn user_of(&self, seat: usize) -> Uuid {
        self.seats[seat]
    }

    pub fn turn_user(&self) -> Uuid {
        self.user_of(self.table().turn)
    }

    /// The cards `user_id` holds, in draw order. Callers must never render
    /// this for anyone but the viewer.
    pub fn hand_of(&self, user_id: Uuid) -> Vec<Card> {
        match self.seat_of(user_id) {
            Some(seat) => {
                let mut table = self.table();
                std::mem::take(&mut table.hands[seat])
            }
            None => Vec::new(),
        }
    }

    pub fn move_count(&self) -> usize {
        self.plays.len()
    }

    /// Cards left to draw, the trump included.
    pub fn stock_remaining(&self) -> usize {
        DRAWS - self.table().drawn
    }

    /// Replay the deal and the history into the live table. Removing a
    /// played card from a hand is a set difference, and trick resolution
    /// never consults the hands, so a history hand-edited into nonsense can
    /// only skew the hand display, never the winners or the score.
    pub fn table(&self) -> Table {
        let trump = self.trump();
        let mut hands = [
            self.deck[..HAND].to_vec(),
            self.deck[HAND..HAND * 2].to_vec(),
        ];
        let mut points = [0; 2];
        let mut tricks = [0; 2];
        let mut drawn = 0usize;
        let mut leader = 0usize;
        let mut history = Vec::with_capacity(self.plays.len() / 2);

        for trick in self.plays.chunks(2) {
            let lead = trick[0];
            hands[leader].retain(|card| *card != lead);
            let Some(&answer) = trick.get(1) else {
                // A trick with one card in it: the follower is on the clock.
                return Table {
                    hands,
                    points,
                    tricks,
                    turn: other(leader),
                    led: Some((leader, lead)),
                    drawn,
                    history,
                    last: Some((leader, lead)),
                };
            };
            let follower = other(leader);
            hands[follower].retain(|card| *card != answer);
            let winner = if takes_trick(answer, lead, trump.suit) {
                follower
            } else {
                leader
            };
            let won = lead.points() + answer.points();
            points[winner] += won;
            tricks[winner] += 1;
            history.push(TrickRecord {
                leader,
                lead,
                answer,
                winner,
                points: won,
            });
            // The winner draws first, then the loser, then the winner leads.
            for seat in [winner, other(winner)] {
                if let Some(card) = self.draw_at(drawn) {
                    hands[seat].push(card);
                    drawn += 1;
                }
            }
            leader = winner;
        }
        let last = history
            .last()
            .map(|trick| (other(trick.leader), trick.answer));
        Table {
            hands,
            points,
            tricks,
            turn: leader,
            led: None,
            drawn,
            history,
            last,
        }
    }

    /// Play one card from the current player's hand. Validates that they
    /// hold it; the caller owns turn order and match status.
    pub fn apply_play(&mut self, card: Card) -> Result<PlayOutcome> {
        let before = self.table();
        let seat = before.turn;
        ensure!(
            before.hands[seat].contains(&card),
            "that card is not in your hand"
        );
        self.plays.push(card);
        let after = self.table();

        // An even play count closes a trick, and the winner is whoever leads
        // the next one.
        let trick = self.plays.len().is_multiple_of(2).then(|| TrickWin {
            winner: after.turn,
            points: after.points[after.turn] - before.points[after.turn],
        });
        let finish = trick.and_then(|_| settle(after.points, self.plays.len()));
        Ok(PlayOutcome {
            seat,
            card,
            trick,
            totals: after.points,
            finish,
        })
    }

    /// The `index`-th card to leave the stock. The trump lies under the
    /// stock, so it is the very last card drawn.
    fn draw_at(&self, index: usize) -> Option<Card> {
        let stock = DECK - STOCK_START;
        if index < stock {
            Some(self.deck[STOCK_START + index])
        } else if index == stock {
            Some(self.trump())
        } else {
            None
        }
    }
}

pub const fn other(seat: usize) -> usize {
    1 - seat
}

/// The finish a completed trick implies. Passing 60 settles the match on the
/// spot, since the rest of the deck can no longer catch it; running out of
/// cards with nobody past 60 means the 120 split evenly.
fn settle(points: [u32; 2], plays: usize) -> Option<MatchEnd> {
    match (0..2).find(|&seat| points[seat] >= WINNING_POINTS) {
        Some(seat) => Some(MatchEnd::Winner(seat)),
        None if plays == DECK => Some(MatchEnd::Draw),
        None => None,
    }
}

/// The follower takes the trick only by out-ranking the led suit or by
/// trumping it. Leading a trump therefore always holds unless it is
/// out-ranked, and an off-suit answer never wins.
fn takes_trick(answer: Card, lead: Card, trump: CardSuit) -> bool {
    if answer.suit == lead.suit {
        answer.rank.beats(lead.rank)
    } else {
        answer.suit == trump
    }
}

fn fresh_deck() -> Vec<Card> {
    SUITS
        .into_iter()
        .flat_map(|suit| Rank::ALL.into_iter().map(move |rank| Card { suit, rank }))
        .collect()
}

#[cfg(test)]
#[path = "briscola_test.rs"]
mod briscola_test;
