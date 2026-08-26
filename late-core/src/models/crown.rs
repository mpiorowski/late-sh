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

use anyhow::{Context, Result};
use chrono::{DateTime, Datelike, NaiveDate, Utc};
use deadpool_postgres::GenericClient;
use serde::{Deserialize, Serialize};
use tokio_postgres::{Client, Row, Transaction};
use uuid::Uuid;

/// Cross-process refresh channel. A take lands on whichever replica the
/// buyer is connected to; every other replica learns about it here. The
/// payload is a [`CrownChange`]: what the deposed holder has to be told,
/// wherever they are connected. The new holder is not in it on purpose; a
/// listener re-reads the open reign rather than trusting a serialized copy.
pub const CROWN_CHANGED_CHANNEL: &str = "crown_changed";

/// What rides the notify. Everything a replica other than the buyer's
/// needs in order to tell the deposed holder who took the crown off them;
/// the glyph itself comes from re-reading the table.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CrownChange {
    pub taker_username: String,
    pub price: i64,
    /// The deposed holder, absent when the crown was vacant.
    pub deposed_user_id: Option<Uuid>,
}

impl CrownChange {
    /// Parse a notify payload. A payload this cannot read is a bug on the
    /// sending side, not a reason to skip the re-read, so the caller logs it
    /// and refreshes anyway.
    pub fn parse(payload: &str) -> Result<Self> {
        serde_json::from_str(payload).context("parsing crown_changed payload")
    }
}

pub async fn listen_for_crown_changes(client: &Client) -> Result<()> {
    client
        .batch_execute(&format!("LISTEN {CROWN_CHANGED_CHANNEL};"))
        .await?;
    Ok(())
}

/// What a vacant crown costs, and the floor every ratchet is clamped to.
/// Decided in SHOP.md's fixed-numbers table: a Bronze gild's price, low
/// enough that the month's race starts on day one (a fresh account can
/// claim it), because the 1.5x ratchet, not the floor, is what makes the
/// crown expensive.
pub const CROWN_MIN_PRICE: i64 = 500;

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
    /// The first of the UTC month the reign was taken in, stamped by the
    /// database from the same clock as `taken_at` so the two can never
    /// disagree about which month a take landed in.
    pub month: NaiveDate,
    pub holder_user_id: Uuid,
    pub paid_chips: i64,
    pub taken_at: DateTime<Utc>,
    /// When the row was closed, which is the next take, not the moment the
    /// reign stopped counting: a reign left open across the month rollover
    /// keeps `ended_at IS NULL` until someone takes the vacant crown, days or
    /// weeks later. Anything reading this as history (a future crown page)
    /// wants `LEAST(ended_at, month + interval '1 month')`.
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
    /// with a raw constraint violation instead of paying the next rung. The
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

    /// Open a reign for `holder_user_id` at the price they paid. The month
    /// is derived in SQL from the same `current_timestamp` that stamps
    /// `taken_at`: computing it on the app clock before the pool checkout
    /// and the advisory-lock wait could stamp a take that crossed midnight
    /// on the last of the month with a month that is already over, and a
    /// reign born stale burns the chips for nothing. The caller closes the
    /// previous reign first; the unique index is what catches it if they
    /// forget.
    pub async fn open_in_tx(
        tx: &Transaction<'_>,
        holder_user_id: Uuid,
        paid_chips: i64,
    ) -> Result<Self> {
        let row = tx
            .query_one(
                "INSERT INTO crown_reigns (month, holder_user_id, paid_chips)
                 VALUES (
                    date_trunc('month', current_timestamp AT TIME ZONE 'UTC')::date,
                    $1,
                    $2
                 )
                 RETURNING *",
                &[&holder_user_id, &paid_chips],
            )
            .await?;
        Ok(Self::from(row))
    }

    /// Tell every replica the crown moved. Sent inside the take transaction,
    /// so Postgres delivers it on commit and never for a rolled-back take.
    pub async fn notify_changed(tx: &Transaction<'_>, change: &CrownChange) -> Result<()> {
        let payload = serde_json::to_string(change).context("encoding crown_changed payload")?;
        tx.execute(
            "SELECT pg_notify($1, $2)",
            &[&CROWN_CHANGED_CHANNEL, &payload],
        )
        .await?;
        Ok(())
    }
}
