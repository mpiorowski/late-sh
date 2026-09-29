//! Cribbage rules for daily correspondence matches. Pure state + logic, no
//! I/O: the service persists `DailyCribbageState` as the match's `state`
//! JSON the same way briscola persists its own.
//!
//! The state stores the deals and the moves, nothing else. Hands, the crib,
//! the starter, the pegging count, whose turn it is, and both scores are all
//! derived by replaying the moves over the deals, so the state can never
//! self-contradict. Each hand is its own shuffle: the service appends a
//! fresh deck when a hand is counted out (`deal_next`), the backgammon
//! pattern of durable server-side randomness, so a client applying a move
//! optimistically never deals.
//!
//! Hidden information is a RENDERING rule, exactly as in briscola: the state
//! holds both hands, the crib, and the starter before it is cut, and
//! `cribbage_ui` is the only thing keeping them off the screen.
//!
//! v1 rules: two players, six cards each, two from each to the dealer's
//! crib. The deal alternates, seat 0 dealing the first hand. The non-dealer
//! (pone) discards first, then the dealer; the starter is cut once both have
//! (a jack pegs the dealer two for his heels). Pegging runs to thirty-one:
//! fifteen and thirty-one peg two, pairs two/six/twelve, runs their length.
//! A player who cannot play says go automatically; the last card before the
//! count resets pegs one (unless it made thirty-one). Then the show, in
//! order: pone's hand, the dealer's hand, the crib, each with the starter.
//! First to 121 wins on the spot, mid-pegging or mid-show, so the count
//! stops the moment somebody pegs out. There is no draw.

use anyhow::{Context, Result, bail, ensure};
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use super::std_deck::{self, Card, Rank};

pub const WINNING_SCORE: u32 = 121;
/// Cards dealt to each player.
pub const DEALT: usize = 6;
/// Cards each player keeps after the discard.
pub const KEPT: usize = 4;
/// The count pegging never passes.
pub const MAX_COUNT: u32 = 31;
/// Deal layout: pone's six, the dealer's six, then the starter. The rest of
/// the deck is never seen.
const STARTER_INDEX: usize = DEALT * 2;
const STATE_VERSION: u8 = 1;

/// One player action. A discard is two cards at once: the crib is built in
/// two moves, one per player, never card by card.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CribbageMove {
    Discard([Card; 2]),
    Play(Card),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DailyCribbageState {
    pub version: u8,
    #[serde(default)]
    pub revision: u64,
    /// Seat 0 deals the first hand; the claim-time coin flip picks who sits
    /// there. The deal alternates from then on.
    pub seats: [Uuid; 2],
    /// One whole shuffled deck per hand, appended by the service when a
    /// hand is counted out.
    pub deals: Vec<Vec<Card>>,
    /// Every move of the match in order, across hands.
    pub moves: Vec<CribbageMove>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// This seat owes two cards to the crib.
    Discard(usize),
    /// This seat plays the next card.
    Peg(usize),
    /// The hand is counted out and the next deal has not been appended yet.
    /// Only a state between the service's apply and its `deal_next` (or a
    /// client's optimistic copy) sits here.
    AwaitingDeal,
    /// This seat reached 121.
    Won(usize),
}

/// Which part of the show a count belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShowPart {
    PoneHand,
    DealerHand,
    Crib,
}

impl ShowPart {
    pub const fn label(self) -> &'static str {
        match self {
            Self::PoneHand => "pone's hand",
            Self::DealerHand => "dealer's hand",
            Self::Crib => "crib",
        }
    }
}

/// Why points were pegged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Why {
    HisHeels,
    Fifteen,
    ThirtyOne,
    /// Cards in the pair: 2, 3 (pair royal), or 4 (double pair royal).
    Pair(u8),
    /// Cards in the run.
    Run(u8),
    Go,
    LastCard,
    Show(ShowPart),
}

impl Why {
    pub fn label(self) -> String {
        match self {
            Self::HisHeels => "his heels".to_string(),
            Self::Fifteen => "fifteen".to_string(),
            Self::ThirtyOne => "thirty-one".to_string(),
            Self::Pair(2) => "a pair".to_string(),
            Self::Pair(3) => "pair royal".to_string(),
            Self::Pair(_) => "double pair royal".to_string(),
            Self::Run(cards) => format!("a run of {cards}"),
            Self::Go => "go".to_string(),
            Self::LastCard => "last card".to_string(),
            Self::Show(part) => part.label().to_string(),
        }
    }
}

/// One scoring event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Peg {
    /// The hand it happened in, from zero.
    pub hand: usize,
    pub seat: usize,
    pub points: u32,
    pub why: Why,
}

/// A counted hand, broken down the way a player counts it aloud.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Count {
    pub fifteens: u32,
    pub pairs: u32,
    pub runs: u32,
    pub flush: u32,
    pub nobs: u32,
}

impl Count {
    pub const fn total(self) -> u32 {
        self.fifteens + self.pairs + self.runs + self.flush + self.nobs
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShownHand {
    pub part: ShowPart,
    pub seat: usize,
    pub cards: [Card; KEPT],
    pub count: Count,
}

/// A hand's show, public once counted. Parts stop where somebody pegged out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Show {
    pub hand: usize,
    pub starter: Card,
    pub parts: Vec<ShownHand>,
}

/// Everything the deals plus the moves imply. Rebuilt on demand; the
/// renderer must read only the viewer's own entry in `hands` and `kept`,
/// and never `crib` before it is shown.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Table {
    pub scores: [u32; 2],
    /// The hand in play (or the last one, once the match is won), from zero.
    pub hand: usize,
    pub dealer: usize,
    pub phase: Phase,
    /// Cards still in hand: six before the discard, then the unplayed ones.
    pub hands: [Vec<Card>; 2],
    /// The four each player kept, for the show.
    pub kept: [Vec<Card>; 2],
    pub crib: Vec<Card>,
    /// Turned up once both players have discarded.
    pub starter: Option<Card>,
    /// The pegging count, zero between counts.
    pub count: u32,
    /// Cards on the current count, in play order, with who played them.
    pub run: Vec<(usize, Card)>,
    /// Every card pegged this hand, in play order.
    pub pegged: Vec<(usize, Card)>,
    /// Every score of the match, in order.
    pub log: Vec<Peg>,
    /// The most recent show, kept up through the next hand so a player
    /// coming back to a fresh deal can still read how the last one counted.
    pub last_show: Option<Show>,
    /// The starter for this hand, face down until both have discarded.
    cut: Card,
    /// Who played the last card, for go and last card.
    last_player: Option<usize>,
}

/// What one move did, for the service and the move feed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MoveOutcome {
    pub seat: usize,
    pub played: CribbageMove,
    /// Every score the move set off, whoever pegged it: a last card can run
    /// straight into the show.
    pub pegs: Vec<Peg>,
    pub scores: [u32; 2],
    pub phase: Phase,
}

impl MoveOutcome {
    /// `two to the crib` / `7♠` / `7♠, fifteen and a pair for 4` /
    /// `5♥, thirty-one for 2, hand over`. A discard never names its cards:
    /// the feed is public and the crib is not.
    pub fn label(&self) -> String {
        let mut label = match self.played {
            CribbageMove::Discard(_) => "two to the crib".to_string(),
            CribbageMove::Play(card) => {
                let mine: Vec<&Peg> = self
                    .pegs
                    .iter()
                    .filter(|peg| peg.seat == self.seat && !matches!(peg.why, Why::Show(_)))
                    .collect();
                if mine.is_empty() {
                    card.label()
                } else {
                    let reasons: Vec<String> = mine.iter().map(|peg| peg.why.label()).collect();
                    let points: u32 = mine.iter().map(|peg| peg.points).sum();
                    format!("{}, {} for {points}", card.label(), reasons.join(" and "))
                }
            }
        };
        match self.phase {
            Phase::Won(_) => label.push_str(", pegged out"),
            Phase::AwaitingDeal => label.push_str(", hand over"),
            Phase::Discard(_) | Phase::Peg(_) => {}
        }
        label
    }
}

impl DailyCribbageState {
    pub fn new(challenger: Uuid, claimer: Uuid, rng: &mut impl Rng) -> Self {
        let seats = if rng.gen_bool(0.5) {
            [challenger, claimer]
        } else {
            [claimer, challenger]
        };
        Self {
            version: STATE_VERSION,
            revision: 0,
            seats,
            deals: vec![std_deck::shuffled_deck(rng)],
            moves: Vec::new(),
        }
    }

    pub fn parse(value: &Value) -> Result<Self> {
        let state: Self =
            serde_json::from_value(value.clone()).context("corrupt daily match state")?;
        ensure!(
            state.version == STATE_VERSION,
            "unsupported daily cribbage state version: {}",
            state.version
        );
        ensure!(!state.deals.is_empty(), "daily cribbage has no deal");
        for deal in &state.deals {
            ensure!(
                std_deck::is_whole_deck(deal),
                "daily cribbage deal is not a deck"
            );
        }
        // Every move must replay, and every deal must be reached: this is
        // the check that lets `table` trust the history afterwards.
        let table = state.replay()?;
        ensure!(
            table.hand + 1 == state.deals.len(),
            "daily cribbage has a deal nobody played"
        );
        Ok(state)
    }

    pub fn seat_of(&self, user_id: Uuid) -> Option<usize> {
        self.seats.iter().position(|&seat| seat == user_id)
    }

    pub fn user_of(&self, seat: usize) -> Uuid {
        self.seats[seat]
    }

    /// Who owes the next move; `None` once the match is won or while the
    /// next deal is still to come.
    pub fn turn_user(&self) -> Option<Uuid> {
        match self.table().phase {
            Phase::Discard(seat) | Phase::Peg(seat) => Some(self.user_of(seat)),
            Phase::AwaitingDeal | Phase::Won(_) => None,
        }
    }

    pub fn move_count(&self) -> usize {
        self.moves.len()
    }

    /// The live table. The history was replayed once at the boundary
    /// (`parse`, or `apply_move` for each move added since), so a failure
    /// here is a bug, not bad input.
    pub fn table(&self) -> Table {
        self.replay()
            .expect("daily cribbage history was validated when it was built")
    }

    /// One move by whoever owes it. Validates the move against the replayed
    /// table; the caller owns turn order and match status.
    pub fn apply_move(&mut self, played: CribbageMove) -> Result<MoveOutcome> {
        let mut table = self.table();
        let seat = match table.phase {
            Phase::Discard(seat) | Phase::Peg(seat) => seat,
            Phase::AwaitingDeal => bail!("the next hand has not been dealt"),
            Phase::Won(_) => bail!("the match is over"),
        };
        let logged = table.log.len();
        table.step(played)?;
        self.moves.push(played);
        Ok(MoveOutcome {
            seat,
            played,
            pegs: table.log[logged..].to_vec(),
            scores: table.scores,
            phase: table.phase,
        })
    }

    /// Append the next hand's shuffle. Server-only: a client never deals.
    pub fn deal_next(&mut self, rng: &mut impl Rng) -> Result<()> {
        ensure!(
            self.table().phase == Phase::AwaitingDeal,
            "the hand in play is not over"
        );
        self.deals.push(std_deck::shuffled_deck(rng));
        Ok(())
    }

    fn replay(&self) -> Result<Table> {
        let mut moves = self.moves.iter();
        let mut table = Table::deal(0, &self.deals[0], [0, 0], Vec::new(), None);
        loop {
            match table.phase {
                Phase::Discard(_) | Phase::Peg(_) => match moves.next() {
                    Some(&played) => table.step(played)?,
                    None => return Ok(table),
                },
                Phase::Won(_) => {
                    ensure!(
                        moves.next().is_none(),
                        "daily cribbage has moves after the match was won"
                    );
                    ensure!(
                        table.hand + 1 == self.deals.len(),
                        "daily cribbage was dealt after the match was won"
                    );
                    return Ok(table);
                }
                Phase::AwaitingDeal => match self.deals.get(table.hand + 1) {
                    Some(deal) => {
                        table = Table::deal(
                            table.hand + 1,
                            deal,
                            table.scores,
                            std::mem::take(&mut table.log),
                            table.last_show.take(),
                        );
                    }
                    None => {
                        ensure!(
                            moves.next().is_none(),
                            "daily cribbage has moves past the last deal"
                        );
                        return Ok(table);
                    }
                },
            }
        }
    }
}

impl Table {
    fn deal(
        hand: usize,
        deal: &[Card],
        scores: [u32; 2],
        log: Vec<Peg>,
        last_show: Option<Show>,
    ) -> Self {
        let dealer = hand % 2;
        let pone = other(dealer);
        let mut hands = [Vec::new(), Vec::new()];
        hands[pone] = deal[..DEALT].to_vec();
        hands[dealer] = deal[DEALT..DEALT * 2].to_vec();
        Self {
            scores,
            hand,
            dealer,
            phase: Phase::Discard(pone),
            hands,
            kept: [Vec::new(), Vec::new()],
            crib: Vec::new(),
            starter: None,
            count: 0,
            run: Vec::new(),
            pegged: Vec::new(),
            log,
            last_show,
            cut: deal[STARTER_INDEX],
            last_player: None,
        }
    }

    pub fn pone(&self) -> usize {
        other(self.dealer)
    }

    /// Whether `seat` holds a card that fits under thirty-one.
    pub fn can_play(&self, seat: usize) -> bool {
        self.hands[seat]
            .iter()
            .any(|card| self.count + card.value() <= MAX_COUNT)
    }

    fn step(&mut self, played: CribbageMove) -> Result<()> {
        match (self.phase, played) {
            (Phase::Discard(seat), CribbageMove::Discard([first, second])) => {
                ensure!(first != second, "pick two different cards for the crib");
                ensure!(
                    self.hands[seat].contains(&first) && self.hands[seat].contains(&second),
                    "that card is not in your hand"
                );
                self.hands[seat].retain(|card| *card != first && *card != second);
                self.crib.extend([first, second]);
                self.kept[seat] = self.hands[seat].clone();
                if seat == self.pone() {
                    self.phase = Phase::Discard(self.dealer);
                } else {
                    self.starter = Some(self.cut);
                    self.phase = Phase::Peg(self.pone());
                    if self.cut.rank == Rank::Jack {
                        self.peg(self.dealer, 2, Why::HisHeels);
                    }
                }
                Ok(())
            }
            (Phase::Peg(seat), CribbageMove::Play(card)) => {
                ensure!(
                    self.hands[seat].contains(&card),
                    "that card is not in your hand"
                );
                ensure!(
                    self.count + card.value() <= MAX_COUNT,
                    "that card would take the count past 31"
                );
                self.hands[seat].retain(|held| *held != card);
                self.count += card.value();
                self.run.push((seat, card));
                self.pegged.push((seat, card));
                self.last_player = Some(seat);
                for (points, why) in pegging_points(&self.run, self.count) {
                    if self.peg(seat, points, why) {
                        return Ok(());
                    }
                }
                if self.count == MAX_COUNT {
                    self.count = 0;
                    self.run.clear();
                }
                self.advance(seat);
                Ok(())
            }
            (Phase::Discard(_), CribbageMove::Play(_)) => {
                bail!("discard two cards to the crib first")
            }
            (Phase::Peg(_), CribbageMove::Discard(_)) => bail!("the crib is already full"),
            (Phase::AwaitingDeal, _) => bail!("the next hand has not been dealt"),
            (Phase::Won(_), _) => bail!("the match is over"),
        }
    }

    /// Hand the play on after `last` put a card down. A player who cannot
    /// play says go without being asked; once neither can, the last card of
    /// the count pegs one and the count starts again with the player after
    /// it. When both hands are empty the pegging is over and the show runs.
    fn advance(&mut self, last: usize) {
        loop {
            let next = other(last);
            if self.can_play(next) {
                self.phase = Phase::Peg(next);
                return;
            }
            if self.can_play(last) {
                self.phase = Phase::Peg(last);
                return;
            }
            let pegged_out = self.hands.iter().all(Vec::is_empty);
            if self.count > 0 {
                let why = if pegged_out { Why::LastCard } else { Why::Go };
                if self.peg(last, 1, why) {
                    return;
                }
            }
            if pegged_out {
                self.show();
                return;
            }
            self.count = 0;
            self.run.clear();
        }
    }

    /// Count the three hands in order, stopping the moment somebody wins.
    fn show(&mut self) {
        let starter = self.starter.expect("the starter is cut before pegging");
        let parts = [
            (ShowPart::PoneHand, self.pone(), self.kept[self.pone()].clone()),
            (
                ShowPart::DealerHand,
                self.dealer,
                self.kept[self.dealer].clone(),
            ),
            (ShowPart::Crib, self.dealer, self.crib.clone()),
        ];
        let mut show = Show {
            hand: self.hand,
            starter,
            parts: Vec::with_capacity(3),
        };
        for (part, seat, cards) in parts {
            let cards: [Card; KEPT] = cards
                .try_into()
                .expect("every hand and the crib hold four cards at the show");
            let count = count_hand(cards, starter, part == ShowPart::Crib);
            show.parts.push(ShownHand {
                part,
                seat,
                cards,
                count,
            });
            if self.peg(seat, count.total(), Why::Show(part)) {
                self.last_show = Some(show);
                return;
            }
        }
        self.last_show = Some(show);
        self.phase = Phase::AwaitingDeal;
    }

    /// Score `points` for `seat`; true when that pegs them out.
    fn peg(&mut self, seat: usize, points: u32, why: Why) -> bool {
        if points == 0 {
            return false;
        }
        self.scores[seat] += points;
        self.log.push(Peg {
            hand: self.hand,
            seat,
            points,
            why,
        });
        if self.scores[seat] >= WINNING_SCORE {
            self.phase = Phase::Won(seat);
            return true;
        }
        false
    }
}

pub const fn other(seat: usize) -> usize {
    1 - seat
}

/// What the card just added to `run` scores: fifteen or thirty-one, then
/// pairs, then the longest run ending on it.
fn pegging_points(run: &[(usize, Card)], count: u32) -> Vec<(u32, Why)> {
    let mut points = Vec::new();
    if count == 15 {
        points.push((2, Why::Fifteen));
    }
    if count == MAX_COUNT {
        points.push((2, Why::ThirtyOne));
    }
    let last = run.last().expect("a card was just played").1.rank;
    let same = run
        .iter()
        .rev()
        .take_while(|(_, card)| card.rank == last)
        .count();
    if same >= 2 {
        // Every pair among the matching cards scores two.
        points.push(((same * (same - 1)) as u32, Why::Pair(same as u8)));
    }
    for length in (3..=run.len()).rev() {
        let mut ranks: Vec<u8> = run[run.len() - length..]
            .iter()
            .map(|(_, card)| card.rank.number())
            .collect();
        ranks.sort_unstable();
        if ranks.windows(2).all(|pair| pair[1] == pair[0] + 1) {
            points.push((length as u32, Why::Run(length as u8)));
            break;
        }
    }
    points
}

/// Count one hand (or the crib) with the starter. A four-card flush scores
/// in hand but not in the crib, where only all five matching counts.
pub fn count_hand(cards: [Card; KEPT], starter: Card, crib: bool) -> Count {
    let all = [cards[0], cards[1], cards[2], cards[3], starter];
    let mut count = Count::default();

    for mask in 1u8..(1 << all.len()) {
        let sum: u32 = (0..all.len())
            .filter(|&index| mask & (1 << index) != 0)
            .map(|index| all[index].value())
            .sum();
        if sum == 15 {
            count.fifteens += 2;
        }
    }

    for (index, card) in all.iter().enumerate() {
        for later in &all[index + 1..] {
            if card.rank == later.rank {
                count.pairs += 2;
            }
        }
    }

    // The longest run length wins; every distinct way of making it scores.
    for length in (3..=all.len()).rev() {
        let runs = (1u8..(1 << all.len()))
            .filter(|mask| mask.count_ones() as usize == length)
            .filter(|&mask| {
                let mut ranks: Vec<u8> = (0..all.len())
                    .filter(|&index| mask & (1 << index) != 0)
                    .map(|index| all[index].rank.number())
                    .collect();
                ranks.sort_unstable();
                ranks.windows(2).all(|pair| pair[1] == pair[0] + 1)
            })
            .count() as u32;
        if runs > 0 {
            count.runs = runs * length as u32;
            break;
        }
    }

    if cards.iter().all(|card| card.suit == cards[0].suit) {
        count.flush = match (starter.suit == cards[0].suit, crib) {
            (true, _) => 5,
            (false, false) => 4,
            (false, true) => 0,
        };
    }

    if cards
        .iter()
        .any(|card| card.rank == Rank::Jack && card.suit == starter.suit)
    {
        count.nobs = 1;
    }
    count
}

#[cfg(test)]
#[path = "cribbage_test.rs"]
mod cribbage_test;
