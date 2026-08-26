//! The crown: one slot, one holder, and a glyph after their name in chat
//! until someone pays more than they did.
//!
//! Every take burns the whole price. There is a `chip_crown_taken` debit and
//! no credit anywhere, so the ladder is funded entirely out of the money
//! supply and its next rung is whatever the last holder was willing to pay
//! (see [`next_price`]). Nobody tunes the number.
//!
//! This module owns every read and write of `crown_reigns`. The chips move
//! through `chips.rs`; the transaction that does both belongs to the caller.

use anyhow::Result;
use chrono::{DateTime, Datelike, Duration, NaiveDate, Utc};
use deadpool_postgres::GenericClient;
use tokio_postgres::{Client, Row, Transaction};
use uuid::Uuid;

/// Cross-process refresh channel. A take lands on whichever replica the
/// buyer is connected to; every other replica learns about it here. The
/// payload is empty on purpose: a listener re-reads the open reign rather
/// than trusting a serialized copy of it.
pub const CROWN_CHANGED_CHANNEL: &str = "crown_changed";

pub async fn listen_for_crown_changes(client: &Client) -> Result<()> {
    client
        .batch_execute(&format!("LISTEN {CROWN_CHANGED_CHANNEL};"))
        .await?;
    Ok(())
}

/// What a vacant crown costs, and the floor every ratchet is clamped to.
/// Decided in SHOP.md's fixed-numbers table.
pub const CROWN_MIN_PRICE: i64 = 5_000;

/// How long a fresh reign is untakeable. The hold is the whole throttle on
/// the #lounge line: every takeover posts, so without it two people with
/// chips could turn the feed into a ticker.
pub const CROWN_HOLD_MINUTES: i64 = 30;

pub fn crown_hold() -> Duration {
    Duration::minutes(CROWN_HOLD_MINUTES)
}

/// What the next taker owes, given what the current holder paid. `None` is a
/// vacant crown (nobody holds it, or the month rolled over), which always
/// costs the minimum: the ladder is a month-long contest, not a permanent
/// one.
pub fn next_price(paid_chips: Option<i64>) -> i64 {
    match paid_chips {
        None => CROWN_MIN_PRICE,
        Some(paid) => {
            // ceil(paid * 1.5) in integers, saturating so an absurd balance
            // cannot wrap the ladder back down into affordable territory.
            let tripled = paid.saturating_mul(3);
            (tripled / 2 + tripled % 2).max(CROWN_MIN_PRICE)
        }
    }
}

/// The first of the UTC month `now` falls in: the `month` a reign taken now
/// belongs to, and the value every "is this reign still current" check
/// compares against.
pub fn crown_month(now: DateTime<Utc>) -> NaiveDate {
    now.date_naive()
        .with_day(1)
        .expect("every valid date has a first day of its month")
}

/// One reign: who held the crown, what they paid for it, and when it started
/// and ended. The live reign is the row with `ended_at IS NULL`, of which the
/// table permits exactly one (migration 156).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CrownReign {
    pub id: Uuid,
    pub month: NaiveDate,
    pub holder_user_id: Uuid,
    pub paid_chips: i64,
    pub taken_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
}

impl From<Row> for CrownReign {
    fn from(row: Row) -> Self {
        Self {
            id: row.get("id"),
            month: row.get("month"),
            holder_user_id: row.get("holder_user_id"),
            paid_chips: row.get("paid_chips"),
            taken_at: row.get("taken_at"),
            ended_at: row.get("ended_at"),
        }
    }
}

impl CrownReign {
    /// Whether this reign still counts. An open reign from a previous UTC
    /// month is stale: the crown reads as vacant at the minimum price from
    /// the rollover onwards, and the next take closes the row. This is how
    /// the month boundary is enforced without a sweeper, the way rentals
    /// expire at read.
    pub fn is_current(&self, now: DateTime<Utc>) -> bool {
        self.ended_at.is_none() && self.month == crown_month(now)
    }

    /// Whether a fresh reign is still inside its untakeable window.
    pub fn is_held(&self, now: DateTime<Utc>) -> bool {
        now < self.taken_at + crown_hold()
    }

    /// Seconds until this reign can be taken, zero once the hold is over.
    pub fn hold_remaining_secs(&self, now: DateTime<Utc>) -> i64 {
        (self.taken_at + crown_hold() - now).num_seconds().max(0)
    }

    /// The open reign, whatever month it belongs to. Callers decide what a
    /// stale one means through [`Self::is_current`]; this stays a plain read
    /// so `/crown` and the take path see the same row.
    pub async fn find_open(client: &impl GenericClient) -> Result<Option<Self>> {
        let row = client
            .query_opt("SELECT * FROM crown_reigns WHERE ended_at IS NULL", &[])
            .await?;
        Ok(row.map(Self::from))
    }

    /// Serialize every take against every other take, then read the open
    /// reign under a row lock.
    ///
    /// Both locks are load-bearing and neither replaces the other. The
    /// advisory lock is what makes a take from a *vacant* crown exact: there
    /// is no row to take `FOR UPDATE`, so without it two concurrent first
    /// takes would both insert and one would die on the partial unique index
    /// with a raw constraint violation instead of a hold refusal. The
    /// `FOR UPDATE` then keeps the read-then-write on an existing reign
    /// exact for anything that reaches the row outside this path.
    pub async fn lock_open(tx: &Transaction<'_>) -> Result<Option<Self>> {
        tx.query_one(
            "SELECT pg_advisory_xact_lock(hashtextextended($1, 0))",
            &[&CROWN_CHANGED_CHANNEL],
        )
        .await?;
        let row = tx
            .query_opt(
                "SELECT * FROM crown_reigns WHERE ended_at IS NULL FOR UPDATE",
                &[],
            )
            .await?;
        Ok(row.map(Self::from))
    }

    /// Close the open reign, if there is one. Called under
    /// [`Self::lock_open`] for both the takeover case and the stale
    /// month-rollover row, so the partial unique index has room for the
    /// insert that follows.
    pub async fn close_in_tx(tx: &Transaction<'_>, reign_id: Uuid) -> Result<()> {
        tx.execute(
            "UPDATE crown_reigns
             SET ended_at = current_timestamp
             WHERE id = $1 AND ended_at IS NULL",
            &[&reign_id],
        )
        .await?;
        Ok(())
    }

    /// Open a reign for `holder_user_id` in `month` at the price they paid.
    /// The caller closes the previous reign first; the unique index is what
    /// catches it if they forget.
    pub async fn open_in_tx(
        tx: &Transaction<'_>,
        month: NaiveDate,
        holder_user_id: Uuid,
        paid_chips: i64,
    ) -> Result<Self> {
        let row = tx
            .query_one(
                "INSERT INTO crown_reigns (month, holder_user_id, paid_chips)
                 VALUES ($1, $2, $3)
                 RETURNING *",
                &[&month, &holder_user_id, &paid_chips],
            )
            .await?;
        Ok(Self::from(row))
    }

    /// Tell every replica the crown moved. Sent inside the take transaction,
    /// so Postgres delivers it on commit and never for a rolled-back take.
    pub async fn notify_changed(tx: &Transaction<'_>) -> Result<()> {
        tx.execute("SELECT pg_notify($1, '')", &[&CROWN_CHANGED_CHANNEL])
            .await?;
        Ok(())
    }
}
