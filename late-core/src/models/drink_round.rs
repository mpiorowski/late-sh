//! The round: one patron buying a drink for everyone else at the bar.
//!
//! Three rules live here and nowhere else.
//!
//! **What counts as asking.** [`ROUND_PHRASES`] is the closed list of things a
//! patron can say to @bartender to buy the house a round. It is deliberately
//! not a model's judgement: this is the only bartender action that spends more
//! than one drink's worth of chips, and the price scales with the room, so the
//! phrase itself is the confirmation. Two modules read this list: the
//! bartender, to decide, and `chat/slur.rs`, to keep a drunk patron's typing
//! from scrambling the one sentence that moves money. If the two ever read
//! different lists, the feature breaks for exactly the people most likely to
//! use it, so there is one list.
//!
//! **What it costs.** [`ROUND_PRICE_PER_PATRON`] for every credit the round
//! actually granted, burned whole.
//!
//! **What it hands over.** Not a drink: a [`DrinkCredit`], cashed only when the
//! patron walks up and orders one themselves. A pour makes someone type drunk
//! in public (`chat/slur.rs`), and that is not a thing to do to a person who
//! did not ask.

use anyhow::Result;
use chrono::{DateTime, Utc};
use deadpool_postgres::GenericClient;
use tokio_postgres::{Row, Transaction};
use uuid::Uuid;

/// What the buyer pays per patron the round reaches. Matches
/// [`crate::models::drinks::DRINK_PRICE_MIN`], the cheapest thing at the bar:
/// a round is a lot of small kindnesses, not one grand one.
pub const ROUND_PRICE_PER_PATRON: i64 = 100;

/// The buzz a cashed round drink records, regardless of what the bartender
/// named or priced the pour at. Three times what the buyer paid for it, and
/// sized against `drinks::DRUNK_LEVEL_THRESHOLDS`: 300 is exactly the buzzed
/// line, so a sober room that drinks the round moves a visible level rather
/// than shuffling within tipsy. It decays like any other drink, which puts the
/// whole party at about an hour.
pub const ROUND_DRINK_POINTS: i64 = 300;

/// How long an uncashed credit stays good. Long enough to cover a patron who
/// was mid-game when it landed and a night shift that logs in later, short
/// enough that the bar is not indefinitely on the hook for a round bought last
/// week.
pub const ROUND_CREDIT_TTL_HOURS: i64 = 24;

/// Everything a patron can say to buy the house a round, lowercase.
///
/// Every phrase here has to survive `chat/slur.rs` unscrambled, which is why
/// the list is protected there rather than merely matched loosely here. Keep
/// them short and unmistakable: this list is a spending authorization, so a
/// phrase that could turn up in ordinary conversation does not belong on it.
pub const ROUND_PHRASES: &[&str] = &[
    "round for all",
    "round for everyone",
    "round for everybody",
    "round for the house",
    "round for the bar",
    "round on me",
];

/// Byte ranges in `text` covered by a [`ROUND_PHRASES`] entry, in the order
/// they appear.
///
/// Matching is case-insensitive and bounded on both ends by a non-alphanumeric
/// character, so "turn around for all of us" is not an order for drinks. ASCII
/// lowercasing never changes a byte's width, so the ranges index `text` itself
/// and not the lowered copy.
pub fn round_phrase_spans(text: &str) -> Vec<(usize, usize)> {
    let haystack = text.to_ascii_lowercase();
    let mut spans = Vec::new();
    for phrase in ROUND_PHRASES {
        let mut from = 0;
        while let Some(offset) = haystack[from..].find(phrase) {
            let start = from + offset;
            let end = start + phrase.len();
            if is_bounded(&haystack, start, end) {
                spans.push((start, end));
            }
            from = start + 1;
        }
    }
    spans.sort_unstable();
    spans
}

/// Whether the patron asked for a round. The bartender's gate.
///
/// Stricter than [`round_phrase_spans`], which is the slur guard's view and
/// protects the phrase wherever it turns up. An order is a statement, so the
/// sentence the phrase sits in must not run on to a `?`: "how much is a round
/// for everyone?" gets an answer, not a bill. Text inside backticks is never
/// an order either, since a code span is how a patron quotes the words
/// without saying them. Segments alternate outside/inside starting outside,
/// so an unbalanced backtick makes the rest of the message not an order,
/// which is the safe way to be wrong about money.
pub fn contains_round_request(text: &str) -> bool {
    text.split('`').step_by(2).any(|segment| {
        round_phrase_spans(segment)
            .into_iter()
            .any(|(_, end)| !sentence_ends_in_question(segment, end))
    })
}

/// Whether the sentence a phrase ending at `end` belongs to closes with a
/// question mark. The first terminator after the phrase decides; a line break
/// or the end of the text counts as a full stop.
fn sentence_ends_in_question(text: &str, end: usize) -> bool {
    match text[end..]
        .chars()
        .find(|ch| matches!(ch, '.' | '!' | '?' | '\n'))
    {
        Some('?') => true,
        Some(_) | None => false,
    }
}

/// Whether `[start, end)` sits on word boundaries rather than inside a longer
/// word.
fn is_bounded(text: &str, start: usize, end: usize) -> bool {
    let before_ok = match text[..start].chars().next_back() {
        Some(ch) => !ch.is_alphanumeric(),
        None => true,
    };
    let after_ok = match text[end..].chars().next() {
        Some(ch) => !ch.is_alphanumeric(),
        None => true,
    };
    before_ok && after_ok
}

/// A round that was bought. The row carries no total: the `chip_ledger` row
/// keyed on this id is the record of what it cost and, by the price, of how
/// many it reached. The credit rows are not that record: a later round takes
/// over a patron's expired credit in place (see [`DrinkRound::open`]), so an
/// old round's roster shrinks after the fact.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DrinkRound {
    pub id: Uuid,
    /// `None` once the buyer deletes their account; the round and its credits
    /// outlive them.
    pub buyer_user_id: Option<Uuid>,
    pub price_per_patron: i64,
    pub created: DateTime<Utc>,
}

impl From<Row> for DrinkRound {
    fn from(row: Row) -> Self {
        Self {
            id: row.get("id"),
            buyer_user_id: row.get("buyer_user_id"),
            price_per_patron: row.get("price_per_patron"),
            created: row.get("created"),
        }
    }
}

/// A round and the patrons it actually reached. `patron_ids` is what the
/// buyer owes for: never the candidate list, always the credits that landed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoundGrant {
    pub round: DrinkRound,
    pub patron_ids: Vec<Uuid>,
}

impl RoundGrant {
    pub fn patron_count(&self) -> i64 {
        self.patron_ids.len() as i64
    }

    /// What the buyer is charged: one price per credit that landed.
    pub fn total_chips(&self) -> i64 {
        self.patron_count() * self.round.price_per_patron
    }
}

/// An open credit and who is behind it, for the bartender's line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OpenCredit {
    pub round_id: Uuid,
    pub buyer_user_id: Option<Uuid>,
    pub expires_at: DateTime<Utc>,
}

impl From<Row> for OpenCredit {
    fn from(row: Row) -> Self {
        Self {
            round_id: row.get("round_id"),
            buyer_user_id: row.get("buyer_user_id"),
            expires_at: row.get("expires_at"),
        }
    }
}

impl DrinkRound {
    /// Open a round and grant its credits, returning the patrons it reached.
    ///
    /// `candidates` is who was at the bar; the grant skips anyone already
    /// holding an open credit, so a second round on the heels of the first
    /// costs the buyer almost nothing. An expired credit is re-used in place,
    /// since the partial unique index keeps the slot occupied until it is
    /// cashed. `patron_ids` comes from the insert's own `RETURNING`, not from
    /// a count taken beforehand, so what the caller charges for and what the
    /// bar actually poured cannot disagree even when two rounds race.
    ///
    /// The chips move in `chips.rs`; the caller owns the transaction that
    /// makes the grant and the charge atomic.
    pub async fn open(
        tx: &Transaction<'_>,
        buyer_user_id: Uuid,
        price_per_patron: i64,
        candidates: &[Uuid],
        ttl_hours: i64,
    ) -> Result<RoundGrant> {
        let row = tx
            .query_one(
                "INSERT INTO drink_rounds (buyer_user_id, price_per_patron)
                 VALUES ($1, $2)
                 RETURNING *",
                &[&buyer_user_id, &price_per_patron],
            )
            .await?;
        let round = Self::from(row);

        let candidates = candidates.to_vec();
        let rows = tx
            .query(
                "INSERT INTO drink_credits (round_id, user_id, expires_at)
                 SELECT $1, patron, current_timestamp + make_interval(hours => $3::int)
                 FROM unnest($2::uuid[]) AS patron
                 ON CONFLICT (user_id) WHERE cashed_at IS NULL DO UPDATE SET
                     round_id = EXCLUDED.round_id,
                     expires_at = EXCLUDED.expires_at,
                     created = current_timestamp
                 WHERE drink_credits.expires_at <= current_timestamp
                 RETURNING user_id",
                &[&round.id, &candidates, &(ttl_hours as i32)],
            )
            .await?;

        Ok(RoundGrant {
            round,
            patron_ids: rows.into_iter().map(|row| row.get("user_id")).collect(),
        })
    }
}

pub struct DrinkCredit;

impl DrinkCredit {
    /// The patron's open credit, if the bar owes them a drink right now.
    /// Read before pouring so the bartender's line can name who bought it.
    pub async fn find_open(
        client: &impl GenericClient,
        user_id: Uuid,
    ) -> Result<Option<OpenCredit>> {
        let row = client
            .query_opt(
                "SELECT c.round_id, c.expires_at, r.buyer_user_id
                 FROM drink_credits c
                 JOIN drink_rounds r ON r.id = c.round_id
                 WHERE c.user_id = $1
                   AND c.cashed_at IS NULL
                   AND c.expires_at > current_timestamp",
                &[&user_id],
            )
            .await?;
        Ok(row.map(OpenCredit::from))
    }

    /// Spend the patron's open credit on the drink in front of them.
    ///
    /// One guarded UPDATE, so two orders landing together cash it once and the
    /// loser pays for their own drink. `None` means there was nothing to
    /// spend: never granted, already cashed, or expired between the read and
    /// the pour.
    pub async fn cash(client: &impl GenericClient, user_id: Uuid) -> Result<Option<OpenCredit>> {
        let row = client
            .query_opt(
                "WITH cashed AS (
                    UPDATE drink_credits
                    SET cashed_at = current_timestamp
                    WHERE user_id = $1
                      AND cashed_at IS NULL
                      AND expires_at > current_timestamp
                    RETURNING round_id, expires_at
                 )
                 SELECT cashed.round_id, cashed.expires_at, r.buyer_user_id
                 FROM cashed
                 JOIN drink_rounds r ON r.id = cashed.round_id",
                &[&user_id],
            )
            .await?;
        Ok(row.map(OpenCredit::from))
    }
}
