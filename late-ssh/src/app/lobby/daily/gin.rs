//! Gin rummy rules for daily correspondence matches. Pure state + logic, no
//! I/O: the service persists `DailyGinState` as the match's `state` JSON.
//!
//! Same shape as cribbage: the deals and the moves, nothing else. Hands, the
//! discard pile, the stock, whose turn it is, and the score are derived by
//! replay, and each hand's shuffle is appended by the service when the hand
//! ends (`deal_next`), so a client applying a move optimistically never
//! deals. The state holds both hands and the whole stock; `gin_ui` is the
//! only thing keeping them off the screen.
//!
//! A turn is two moves, a draw and then a discard, because the draw has to
//! be committed before the drawn card is seen: a player who could look at
//! the top of the stock and then change their mind to the discard pile
//! would be peeking. The service keeps the turn's clock running across the
//! two, so a turn is still one day.
//!
//! v1 rules: ten cards each, the next card turned up to start the discard
//! pile. The deal alternates, seat 0 dealing the first hand, and the
//! non-dealer plays first (straight into an ordinary turn: the table game's
//! offer of the first upcard is left out). Draw from the stock or take the
//! top discard, then throw one; the card just taken from the pile cannot go
//! straight back. Discarding with ten or fewer points of deadwood may knock;
//! no deadwood is gin. Melds and layoffs are found for both players, never
//! declared: the arrangement that leaves the least deadwood is the one that
//! counts. A knock scores the difference in deadwood after the defender
//! lays off; a defender at or under the knocker undercuts them for the
//! difference plus 25; gin scores 25 plus the defender's deadwood with no
//! layoffs. The hand is dead, nobody scoring, when the stock is down to two
//! cards. First to 100 wins the match; the line and box bonuses of the
//! table game are left out, since the chip prize is fixed. There is no draw.

use anyhow::{Context, Result, bail, ensure};
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use super::std_deck::{self, Card};

pub const HAND: usize = 10;
pub const TARGET_SCORE: u32 = 100;
pub const GIN_BONUS: u32 = 25;
pub const UNDERCUT_BONUS: u32 = 25;
/// The most deadwood a knock may carry.
pub const MAX_KNOCK: u32 = 10;
/// The hand is dead once the stock is down to this many cards.
pub const DEAD_STOCK: usize = 2;
/// Deal layout: pone's ten, the dealer's ten, the upcard, then the stock in
/// draw order.
const UPCARD_INDEX: usize = HAND * 2;
const STOCK_START: usize = UPCARD_INDEX + 1;
/// Cards the stock starts with.
pub const STOCK: usize = std_deck::DECK - STOCK_START;
const STATE_VERSION: u8 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Pile {
    Stock,
    Discard,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GinMove {
    Draw(Pile),
    Discard { card: Card, knock: bool },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DailyGinState {
    pub version: u8,
    #[serde(default)]
    pub revision: u64,
    /// Seat 0 deals the first hand; the claim-time coin flip picks who sits
    /// there. The deal alternates from then on.
    pub seats: [Uuid; 2],
    /// One whole shuffled deck per hand, appended by the service when a
    /// hand ends.
    pub deals: Vec<Vec<Card>>,
    /// Every move of the match in order, across hands.
    pub moves: Vec<GinMove>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// This seat draws next.
    Draw(usize),
    /// This seat has drawn and owes a discard.
    Discard(usize),
    /// The hand is over and the next deal has not been appended yet.
    AwaitingDeal,
    /// This seat reached 100.
    Won(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HandEnd {
    Gin,
    Knock,
    Undercut,
    /// The stock ran down with nobody out.
    Dead,
}

/// A hand laid down at the end: public once the hand is over.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reveal {
    pub seat: usize,
    pub melds: Vec<Vec<Card>>,
    /// Defender's cards laid off on the knocker's melds.
    pub laid_off: Vec<Card>,
    pub deadwood: Vec<Card>,
}

impl Reveal {
    pub fn deadwood_points(&self) -> u32 {
        points(&self.deadwood)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HandResult {
    pub hand: usize,
    pub end: HandEnd,
    /// Who scored, and how much. `None` for a dead hand.
    pub scored: Option<(usize, u32)>,
    /// The knocker's hand, then the defender's. Empty for a dead hand.
    pub reveals: Vec<Reveal>,
}

/// Everything the deals plus the moves imply. The renderer must read only
/// the viewer's own entry in `hands`, and never the stock.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Table {
    pub scores: [u32; 2],
    pub hand: usize,
    pub dealer: usize,
    pub phase: Phase,
    pub hands: [Vec<Card>; 2],
    /// Bottom to top; every card in it is public.
    pub discards: Vec<Card>,
    /// The card taken from the pile this turn, which may not go straight
    /// back.
    pub taken: Option<Card>,
    /// Every finished hand, in order.
    pub results: Vec<HandResult>,
    /// The last move and who made it, for the status line. A stock draw is
    /// logged without its card: only the drawer may see it.
    pub last: Option<(usize, GinMove)>,
    /// The rest of the stock, in draw order.
    stock: Vec<Card>,
}

/// What one move did, for the service and the move feed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MoveOutcome {
    pub seat: usize,
    pub played: GinMove,
    /// The card a draw from the pile took; public, since it was face up.
    pub taken: Option<Card>,
    /// Set when the move ended the hand.
    pub result: Option<HandResult>,
    pub scores: [u32; 2],
    pub phase: Phase,
}

impl MoveOutcome {
    /// `drew from the stock` / `took 7♠` / `threw 9♦` / `gin on 9♦ for 31` /
    /// `knocked on 9♦ for 12` / `knocked on 9♦, undercut for 27` /
    /// `threw 9♦, dead hand`, plus `, game` when it won the match. A stock
    /// draw never names its card: the feed is public.
    pub fn label(&self) -> String {
        let mut label = match (self.played, &self.result) {
            (GinMove::Draw(Pile::Stock), _) => "drew from the stock".to_string(),
            (GinMove::Draw(Pile::Discard), _) => {
                let card = self.taken.expect("a draw from the pile takes its top card");
                format!("took {}", card.label())
            }
            (GinMove::Discard { card, .. }, None) => format!("threw {}", card.label()),
            (GinMove::Discard { card, .. }, Some(result)) => match (result.end, result.scored) {
                (HandEnd::Gin, Some((_, points))) => {
                    format!("gin on {} for {points}", card.label())
                }
                (HandEnd::Knock, Some((_, points))) => {
                    format!("knocked on {} for {points}", card.label())
                }
                (HandEnd::Undercut, Some((_, points))) => {
                    format!("knocked on {}, undercut for {points}", card.label())
                }
                (HandEnd::Dead, _) | (_, None) => {
                    format!("threw {}, dead hand", card.label())
                }
            },
        };
        if matches!(self.phase, Phase::Won(_)) {
            label.push_str(", game");
        }
        label
    }
}

impl DailyGinState {
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
            "unsupported daily gin state version: {}",
            state.version
        );
        ensure!(!state.deals.is_empty(), "daily gin has no deal");
        for deal in &state.deals {
            ensure!(
                std_deck::is_whole_deck(deal),
                "daily gin deal is not a deck"
            );
        }
        // Every move must replay and every deal must be reached: this is
        // the check that lets `table` trust the history afterwards.
        let table = state.replay()?;
        ensure!(
            table.hand + 1 == state.deals.len(),
            "daily gin has a deal nobody played"
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
            Phase::Draw(seat) | Phase::Discard(seat) => Some(self.user_of(seat)),
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
            .expect("daily gin history was validated when it was built")
    }

    /// One move by whoever owes it. Validates against the replayed table;
    /// the caller owns turn order and match status.
    pub fn apply_move(&mut self, played: GinMove) -> Result<MoveOutcome> {
        let mut table = self.table();
        let seat = match table.phase {
            Phase::Draw(seat) | Phase::Discard(seat) => seat,
            Phase::AwaitingDeal => bail!("the next hand has not been dealt"),
            Phase::Won(_) => bail!("the match is over"),
        };
        let finished = table.results.len();
        table.step(played)?;
        self.moves.push(played);
        Ok(MoveOutcome {
            seat,
            played,
            taken: match played {
                GinMove::Draw(Pile::Discard) => table.taken,
                GinMove::Draw(Pile::Stock) | GinMove::Discard { .. } => None,
            },
            result: table.results.get(finished).cloned(),
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
        let mut table = Table::deal(0, &self.deals[0], [0, 0], Vec::new());
        loop {
            match table.phase {
                Phase::Draw(_) | Phase::Discard(_) => match moves.next() {
                    Some(&played) => table.step(played)?,
                    None => return Ok(table),
                },
                Phase::Won(_) => {
                    ensure!(
                        moves.next().is_none(),
                        "daily gin has moves after the match was won"
                    );
                    ensure!(
                        table.hand + 1 == self.deals.len(),
                        "daily gin was dealt after the match was won"
                    );
                    return Ok(table);
                }
                Phase::AwaitingDeal => match self.deals.get(table.hand + 1) {
                    Some(deal) => {
                        table = Table::deal(
                            table.hand + 1,
                            deal,
                            table.scores,
                            std::mem::take(&mut table.results),
                        );
                    }
                    None => {
                        ensure!(
                            moves.next().is_none(),
                            "daily gin has moves past the last deal"
                        );
                        return Ok(table);
                    }
                },
            }
        }
    }
}

impl Table {
    fn deal(hand: usize, deal: &[Card], scores: [u32; 2], results: Vec<HandResult>) -> Self {
        let dealer = hand % 2;
        let pone = other(dealer);
        let mut hands = [Vec::new(), Vec::new()];
        hands[pone] = deal[..HAND].to_vec();
        hands[dealer] = deal[HAND..HAND * 2].to_vec();
        Self {
            scores,
            hand,
            dealer,
            phase: Phase::Draw(pone),
            hands,
            discards: vec![deal[UPCARD_INDEX]],
            taken: None,
            results,
            last: None,
            stock: deal[STOCK_START..].to_vec(),
        }
    }

    pub fn stock_remaining(&self) -> usize {
        self.stock.len()
    }

    /// `seat`'s cards melds first, then deadwood: the order the board draws
    /// a hand in and the cursor walks. Only ever the viewer's.
    pub fn held(&self, seat: usize) -> Vec<Card> {
        best_melding(&self.hands[seat]).cards()
    }

    /// The card a player drawing from the pile would take.
    pub fn top_discard(&self) -> Option<Card> {
        self.discards.last().copied()
    }

    fn step(&mut self, played: GinMove) -> Result<()> {
        match (self.phase, played) {
            (Phase::Draw(seat), GinMove::Draw(Pile::Stock)) => {
                // The dead-hand rule ends the hand before the stock can run
                // out, so an empty stock here is a corrupt history.
                ensure!(!self.stock.is_empty(), "the stock is empty");
                let card = self.stock.remove(0);
                self.hands[seat].push(card);
                self.taken = None;
                self.phase = Phase::Discard(seat);
                self.last = Some((seat, played));
                Ok(())
            }
            (Phase::Draw(seat), GinMove::Draw(Pile::Discard)) => {
                let card = self.discards.pop().context("the discard pile is empty")?;
                self.hands[seat].push(card);
                self.taken = Some(card);
                self.phase = Phase::Discard(seat);
                self.last = Some((seat, played));
                Ok(())
            }
            (Phase::Discard(seat), GinMove::Discard { card, knock }) => {
                ensure!(
                    self.hands[seat].contains(&card),
                    "that card is not in your hand"
                );
                ensure!(
                    self.taken != Some(card),
                    "you cannot throw back the card you just took"
                );
                self.hands[seat].retain(|held| *held != card);
                self.discards.push(card);
                self.taken = None;
                self.last = Some((seat, played));
                if knock {
                    let deadwood = best_melding(&self.hands[seat]).deadwood_points();
                    ensure!(
                        deadwood <= MAX_KNOCK,
                        "you need {MAX_KNOCK} or less deadwood to knock, you hold {deadwood}"
                    );
                    self.settle(seat);
                } else if self.stock.len() <= DEAD_STOCK {
                    self.results.push(HandResult {
                        hand: self.hand,
                        end: HandEnd::Dead,
                        scored: None,
                        reveals: Vec::new(),
                    });
                    self.phase = Phase::AwaitingDeal;
                } else {
                    self.phase = Phase::Draw(other(seat));
                }
                Ok(())
            }
            (Phase::Draw(_), GinMove::Discard { .. }) => bail!("draw a card first"),
            (Phase::Discard(_), GinMove::Draw(_)) => bail!("you have already drawn"),
            (Phase::AwaitingDeal, _) => bail!("the next hand has not been dealt"),
            (Phase::Won(_), _) => bail!("the match is over"),
        }
    }

    /// Lay both hands down after `knocker` went out, score the hand, and
    /// either end the match or wait for the next deal.
    fn settle(&mut self, knocker: usize) {
        let defender = other(knocker);
        let knocked = best_melding(&self.hands[knocker]);
        let knocker_points = knocked.deadwood_points();
        let (end, defended, scored) = if knocker_points == 0 {
            // Gin: the defender may not lay off.
            let melding = best_melding(&self.hands[defender]);
            let defended = Reveal {
                seat: defender,
                melds: melding.melds,
                laid_off: Vec::new(),
                deadwood: melding.deadwood,
            };
            let scored = (knocker, GIN_BONUS + defended.deadwood_points());
            (HandEnd::Gin, defended, scored)
        } else {
            let defended = best_defence(&self.hands[defender], &knocked.melds, defender);
            let defender_points = defended.deadwood_points();
            if knocker_points < defender_points {
                let scored = (knocker, defender_points - knocker_points);
                (HandEnd::Knock, defended, scored)
            } else {
                let scored = (defender, knocker_points - defender_points + UNDERCUT_BONUS);
                (HandEnd::Undercut, defended, scored)
            }
        };
        self.scores[scored.0] += scored.1;
        self.results.push(HandResult {
            hand: self.hand,
            end,
            scored: Some(scored),
            reveals: vec![
                Reveal {
                    seat: knocker,
                    melds: knocked.melds,
                    laid_off: Vec::new(),
                    deadwood: knocked.deadwood,
                },
                defended,
            ],
        });
        self.phase = if self.scores[scored.0] >= TARGET_SCORE {
            Phase::Won(scored.0)
        } else {
            Phase::AwaitingDeal
        };
    }
}

pub const fn other(seat: usize) -> usize {
    1 - seat
}

pub fn points(cards: &[Card]) -> u32 {
    cards.iter().map(|card| card.value()).sum()
}

/// A hand split into melds and deadwood.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Melding {
    pub melds: Vec<Vec<Card>>,
    pub deadwood: Vec<Card>,
}

impl Melding {
    pub fn deadwood_points(&self) -> u32 {
        points(&self.deadwood)
    }

    /// Melds first, then deadwood: the order a hand is drawn in and the
    /// board cursor walks, so the two always agree.
    pub fn cards(&self) -> Vec<Card> {
        self.melds
            .iter()
            .flatten()
            .chain(self.deadwood.iter())
            .copied()
            .collect()
    }
}

/// The arrangement leaving the least deadwood, melds sorted low to high and
/// deadwood by rank. Ties go to the first found, which is deterministic.
pub fn best_melding(hand: &[Card]) -> Melding {
    let mut best: Option<(u32, Vec<u16>)> = None;
    each_melding(hand, &mut |chosen, used| {
        let deadwood = unused_points(hand, used);
        if best.as_ref().is_none_or(|(least, _)| deadwood < *least) {
            best = Some((deadwood, chosen.to_vec()));
        }
    });
    let (_, chosen) = best.expect("the empty arrangement is always visited");
    melding_of(hand, &chosen)
}

/// Deadwood left after throwing `card` from an eleven-card hand: what the
/// board shows before a discard, and whether it may knock.
pub fn deadwood_after_discard(hand: &[Card], card: Card) -> u32 {
    let rest: Vec<Card> = hand.iter().copied().filter(|held| *held != card).collect();
    best_melding(&rest).deadwood_points()
}

/// The defender's best answer to a knock: every way to meld, each followed
/// by laying off what it can on the knocker's melds, keeping whichever
/// leaves the least deadwood.
fn best_defence(hand: &[Card], knocker_melds: &[Vec<Card>], seat: usize) -> Reveal {
    let mut best: Option<Reveal> = None;
    each_melding(hand, &mut |chosen, _| {
        let melding = melding_of(hand, chosen);
        let (laid_off, deadwood) = lay_off(knocker_melds, &melding.deadwood);
        let reveal = Reveal {
            seat,
            melds: melding.melds,
            laid_off,
            deadwood,
        };
        if best
            .as_ref()
            .is_none_or(|kept| reveal.deadwood_points() < kept.deadwood_points())
        {
            best = Some(reveal);
        }
    });
    best.expect("the empty arrangement is always visited")
}

/// Lay off onto the knocker's melds until nothing more fits. A card that
/// extends a run goes there before it completes a set, since an extended run
/// can take the next card along and a set of four takes nothing.
fn lay_off(melds: &[Vec<Card>], deadwood: &[Card]) -> (Vec<Card>, Vec<Card>) {
    let mut melds = melds.to_vec();
    let mut remaining = deadwood.to_vec();
    let mut laid_off = Vec::new();
    loop {
        let fit = remaining.iter().enumerate().find_map(|(index, card)| {
            melds
                .iter()
                .position(|meld| extends_run(meld, *card))
                .or_else(|| melds.iter().position(|meld| completes_set(meld, *card)))
                .map(|meld| (index, meld))
        });
        let Some((index, meld)) = fit else {
            break;
        };
        let card = remaining.remove(index);
        melds[meld].push(card);
        laid_off.push(card);
    }
    std_deck::sort_by_rank(&mut remaining);
    (laid_off, remaining)
}

fn is_run(meld: &[Card]) -> bool {
    meld.iter().all(|card| card.suit == meld[0].suit)
}

fn extends_run(meld: &[Card], card: Card) -> bool {
    if !is_run(meld) || card.suit != meld[0].suit {
        return false;
    }
    let low = meld
        .iter()
        .map(|card| card.rank.number())
        .min()
        .expect("melds hold cards");
    let high = meld
        .iter()
        .map(|card| card.rank.number())
        .max()
        .expect("melds hold cards");
    card.rank.number() + 1 == low || card.rank.number() == high + 1
}

fn completes_set(meld: &[Card], card: Card) -> bool {
    !is_run(meld) && meld.len() < 4 && card.rank == meld[0].rank
}

/// Visit every set of non-overlapping melds the hand holds, the empty one
/// included, as bitmasks over the hand's indices. At most eleven cards, so
/// this stays small.
fn each_melding(hand: &[Card], visit: &mut dyn FnMut(&[u16], u16)) {
    let candidates = candidate_melds(hand);
    let mut chosen = Vec::new();
    walk(&candidates, 0, 0, &mut chosen, visit);
}

fn walk(
    candidates: &[u16],
    start: usize,
    used: u16,
    chosen: &mut Vec<u16>,
    visit: &mut dyn FnMut(&[u16], u16),
) {
    visit(chosen, used);
    for index in start..candidates.len() {
        let meld = candidates[index];
        if meld & used == 0 {
            chosen.push(meld);
            walk(candidates, index + 1, used | meld, chosen, visit);
            chosen.pop();
        }
    }
}

/// Every meld the hand could make: three or four of a rank, and every run
/// of three or more in one suit (sub-runs included, so a long run can be
/// split around a set).
fn candidate_melds(hand: &[Card]) -> Vec<u16> {
    let mut melds = Vec::new();
    for rank in std_deck::Rank::ALL {
        let same: Vec<usize> = (0..hand.len()).filter(|&i| hand[i].rank == rank).collect();
        let all = same.iter().fold(0u16, |mask, &i| mask | 1 << i);
        match same.len() {
            3 => melds.push(all),
            4 => {
                melds.push(all);
                // Each three of the four, so one of them can go to a run.
                for &left_out in &same {
                    melds.push(all & !(1 << left_out));
                }
            }
            _ => {}
        }
    }
    for suit in std_deck::SUITS {
        let mut in_suit: Vec<usize> = (0..hand.len()).filter(|&i| hand[i].suit == suit).collect();
        in_suit.sort_by_key(|&i| hand[i].rank);
        for start in 0..in_suit.len() {
            let mut mask = 1u16 << in_suit[start];
            for end in start + 1..in_suit.len() {
                if hand[in_suit[end]].rank.number() != hand[in_suit[end - 1]].rank.number() + 1 {
                    break;
                }
                mask |= 1 << in_suit[end];
                if end - start + 1 >= 3 {
                    melds.push(mask);
                }
            }
        }
    }
    melds
}

fn unused_points(hand: &[Card], used: u16) -> u32 {
    (0..hand.len())
        .filter(|&i| used & (1 << i) == 0)
        .map(|i| hand[i].value())
        .sum()
}

fn melding_of(hand: &[Card], chosen: &[u16]) -> Melding {
    let mut melds: Vec<Vec<Card>> = chosen
        .iter()
        .map(|&mask| {
            let mut meld: Vec<Card> = (0..hand.len())
                .filter(|&i| mask & (1 << i) != 0)
                .map(|i| hand[i])
                .collect();
            std_deck::sort_by_rank(&mut meld);
            meld
        })
        .collect();
    melds.sort_by_key(|meld| (meld[0].rank, std_deck::suit_order(meld[0].suit)));
    let used = chosen.iter().fold(0u16, |all, mask| all | mask);
    let mut deadwood: Vec<Card> = (0..hand.len())
        .filter(|&i| used & (1 << i) == 0)
        .map(|i| hand[i])
        .collect();
    std_deck::sort_by_rank(&mut deadwood);
    Melding { melds, deadwood }
}

#[cfg(test)]
#[path = "gin_test.rs"]
mod gin_test;
