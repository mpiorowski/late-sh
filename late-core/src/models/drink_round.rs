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
//! did not ask. Credits stack up to [`MAX_OPEN_CREDITS`], so a patron who was
//! heads-down through three rounds is owed three drinks rather than one, and
//! the cap rather than the schema is what stops a room being bought for
//! forever.

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
/// named or priced the pour at. Four times what the buyer paid for it, and
/// sized against `drinks::DRUNK_LEVEL_THRESHOLDS`: buzzed starts at 300, so a
/// flat 300 landed exactly on the line and the first decay tick (334 an hour)
/// dropped the drinker back to tipsy within seconds of the pour. 400 buys
/// about eighteen minutes of the level the round is meant to hand out, and is
/// still gone in a bit over an hour like any other drink.
pub const ROUND_DRINK_POINTS: i64 = 400;

/// How many unclaimed drinks one patron may have waiting at once.
///
/// Credits stack (migration 168) so the buyer of the round nobody was around
/// for is not buying air, but not without a ceiling: at
/// [`ROUND_DRINK_POINTS`] a head this banks 1,200 points against the 4,000
/// cap, a real night's drinking and not enough for one buyer to park somebody
/// at wasted. It is also the mechanic's only throttle now that the schema no
/// longer provides one, since a patron at the cap costs the next buyer
/// nothing: a room that will not drink stops being worth buying for.
pub const MAX_OPEN_CREDITS: i64 = 3;

/// How long an uncashed credit stays good. Long enough to cover a patron who
/// was mid-game when it landed and a night shift that logs in later, short
/// enough that the bar is not indefinitely on the hook for a round bought last
/// week.
pub const ROUND_CREDIT_TTL_HOURS: i64 = 24;

/// What every grant serializes on. The cap is counted from rows a concurrent
/// round may be inserting and cannot see, so rounds take this lock before
/// granting and hold it to commit; see [`DrinkRound::open`].
const ROUND_GRANT_LOCK: &str = "drink_round_grant";

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
/// many it reached. The credit rows are the round's roster and nothing more,
/// though since migration 168 they are at least a stable one: a credit belongs
/// to the round that bought it for as long as it exists, where the old
/// one-per-patron scheme let a later round take an expired credit over in
/// place and shrink an old round's roster after the fact.
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

/// A credit that was just spent, and what the patron still has behind it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CashedCredit {
    pub round_id: Uuid,
    pub buyer_user_id: Option<Uuid>,
    /// Open credits still on the patron's tab after this pour. The bartender
    /// says this number out loud, so it is counted by the same statement that
    /// spends the credit rather than read back after it.
    pub remaining: i64,
}

impl From<Row> for CashedCredit {
    fn from(row: Row) -> Self {
        Self {
            round_id: row.get("round_id"),
            buyer_user_id: row.get("buyer_user_id"),
            remaining: row.get("remaining"),
        }
    }
}

impl DrinkRound {
    /// Open a round and grant its credits, returning the patrons it reached.
    ///
    /// `candidates` is who was at the bar; the grant skips anyone already
    /// holding `max_open` unexpired credits, so a round bought into a room
    /// that has not been drinking costs the buyer almost nothing. Expired
    /// credits are neither counted nor re-used: they are rows nobody can
    /// drink, and the only thing that ever mattered about them was that they
    /// used to block the patron's one slot. `patron_ids` comes from the
    /// insert's own `RETURNING`, not from a count taken beforehand, so what
    /// the caller charges for and what the bar actually poured cannot
    /// disagree even when two rounds race.
    ///
    /// Every round takes [`ROUND_GRANT_LOCK`] before granting. The cap is a
    /// read-then-write over rows a concurrent round is inserting and cannot
    /// see, so without it two rounds landing together would both read a
    /// patron at two open credits and both grant a third. Rounds are rare and
    /// the lock covers two statements, so serializing every round against
    /// every other is the cheapest way to make the cap mean the number it
    /// says. It also fixes the order the patrons' rows are locked in, which
    /// two overlapping rounds working from differently ordered presence reads
    /// could otherwise deadlock on.
    ///
    /// The chips move in `chips.rs`; the caller owns the transaction that
    /// makes the grant and the charge atomic.
    pub async fn open(
        tx: &Transaction<'_>,
        buyer_user_id: Uuid,
        price_per_patron: i64,
        candidates: &[Uuid],
        ttl_hours: i64,
        max_open: i64,
    ) -> Result<RoundGrant> {
        tx.query_one(
            "SELECT pg_advisory_xact_lock(hashtextextended($1, 0))",
            &[&ROUND_GRANT_LOCK],
        )
        .await?;

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
                 WHERE (
                     SELECT count(*)
                     FROM drink_credits held
                     WHERE held.user_id = patron
                       AND held.cashed_at IS NULL
                       AND held.expires_at > current_timestamp
                 ) < $4::bigint
                 ON CONFLICT (round_id, user_id) DO NOTHING
                 RETURNING user_id",
                &[&round.id, &candidates, &(ttl_hours as i32), &max_open],
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
    /// The credit the patron would drink next: the one closest to going cold,
    /// out of however many they are holding. Read before pouring so the bar
    /// knows the pour is comped; who bought it comes from [`DrinkCredit::cash`],
    /// which may spend a different credit than was open here.
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
                   AND c.expires_at > current_timestamp
                 ORDER BY c.expires_at, c.created
                 LIMIT 1",
                &[&user_id],
            )
            .await?;
        Ok(row.map(OpenCredit::from))
    }

    /// Spend one of the patron's open credits on the drink in front of them:
    /// the one closest to expiring, so a banked drink is never lost to the
    /// clock while a fresher one sits behind it.
    ///
    /// The row is picked `FOR UPDATE SKIP LOCKED`, so two orders landing
    /// together take two different credits (two orders are two drinks) and
    /// neither waits on the other; with only one credit open the loser takes
    /// nothing and pays for their own drink. `None` means there was nothing to
    /// spend: never granted, all drunk, or expired between the read and the
    /// pour.
    ///
    /// `remaining` excludes the cashed row by id rather than being counted
    /// afterwards, because a data-modifying CTE's write is not visible to the
    /// rest of its own statement. It can still over-count by one in the
    /// two-simultaneous-orders race: the other pour's credit is locked but
    /// not yet committed, so both snapshots still count it. One optimistic
    /// scripted line, self-correcting on the next order; accepted.
    pub async fn cash(client: &impl GenericClient, user_id: Uuid) -> Result<Option<CashedCredit>> {
        let row = client
            .query_opt(
                "WITH cashed AS (
                    UPDATE drink_credits
                    SET cashed_at = current_timestamp
                    WHERE id = (
                        SELECT id
                        FROM drink_credits
                        WHERE user_id = $1
                          AND cashed_at IS NULL
                          AND expires_at > current_timestamp
                        ORDER BY expires_at, created
                        LIMIT 1
                        FOR UPDATE SKIP LOCKED
                    )
                    RETURNING id, round_id
                 )
                 SELECT
                     cashed.round_id,
                     r.buyer_user_id,
                     (SELECT count(*)
                      FROM drink_credits held
                      WHERE held.user_id = $1
                        AND held.cashed_at IS NULL
                        AND held.expires_at > current_timestamp
                        AND held.id <> cashed.id) AS remaining
                 FROM cashed
                 JOIN drink_rounds r ON r.id = cashed.round_id",
                &[&user_id],
            )
            .await?;
        Ok(row.map(CashedCredit::from))
    }
}
