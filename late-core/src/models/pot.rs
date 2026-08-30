//! The pot: one parimutuel raffle a week, drawn on a fixed weekday at a
//! fixed UTC hour.
//!
//! Tickets cost [`POT_TICKET_PRICE`] and are capped at
//! [`POT_MAX_TICKETS_PER_DAY`] per player per UTC day, so the draw is about
//! showing up through the week rather than about bank. At the draw one ticket is
//! pulled weighted by holding, [`payout_for`] of the pot goes to whoever
//! holds it, and the rest has no credit row anywhere: that gap is the burn.
//!
//! This module owns every read and write of `pots` and `pot_tickets`, plus
//! the two pure functions the money depends on ([`payout_for`] and
//! [`draw_from_seed`]). The chips move through `chips.rs`; the transaction
//! that does both belongs to the caller.

use anyhow::{Context, Result};
use chrono::{DateTime, Datelike, Duration, Utc, Weekday};
use serde::{Deserialize, Serialize};
use tokio_postgres::{Client, GenericClient, Row, Transaction};
use uuid::Uuid;

/// Cross-process refresh channel. A buy or a draw lands on one replica; every
/// other replica learns about it here and re-reads the pot. Same shape as
/// `crown_changed`.
pub const POT_CHANGED_CHANNEL: &str = "pot_changed";

/// What one ticket costs. Decided in SHOP.md's fixed-numbers table: cheap
/// enough that an arcade afternoon buys a handful, so the pot fills from the
/// whole clubhouse rather than from three whales.
pub const POT_TICKET_PRICE: i64 = 100;

/// The most tickets one account may buy in one UTC day. The cap is what
/// keeps the draw a raffle instead of an auction, and the day is what makes
/// it a reason to come back: a full week is 70 tickets and nobody can buy
/// them on Monday. 1,000 chips a day is an arcade afternoon, so the cap is
/// reachable by anyone who plays, not only by a six-figure balance.
pub const POT_MAX_TICKETS_PER_DAY: i64 = 10;

/// The weekday and UTC hour the pot draws at, every week. Two constants: the
/// countdown, the sweeper, and the next pot's `draws_at` all derive from
/// them. Monday 21:00 UTC is late evening in Europe and afternoon across the
/// US, the widest overlap the clubhouse has.
pub const POT_DRAW_WEEKDAY: Weekday = Weekday::Mon;
pub const POT_DRAW_HOUR_UTC: u32 = 21;

/// What the winner takes: 80% of what the tickets paid in, floored. The
/// remaining fifth is never re-minted, which is the whole point of the
/// mechanic.
pub fn payout_for(size: i64) -> i64 {
    size.saturating_mul(4) / 5
}

/// The next [`POT_DRAW_WEEKDAY`] at [`POT_DRAW_HOUR_UTC`] strictly after
/// `now`. A draw that settles Monday at 21:00:30 schedules the next pot for
/// the following Monday, and a pot opened on a Thursday draws the coming
/// Monday evening.
pub fn next_draw_at(now: DateTime<Utc>) -> DateTime<Utc> {
    let today = now
        .date_naive()
        .and_hms_opt(POT_DRAW_HOUR_UTC, 0, 0)
        .expect("the draw hour is a valid time of day")
        .and_utc();
    // The first draw hour strictly after now, then forward to the weekday.
    let candidate = match today > now {
        true => today,
        false => today + Duration::days(1),
    };
    let days_ahead = (i64::from(POT_DRAW_WEEKDAY.num_days_from_monday())
        - i64::from(candidate.weekday().num_days_from_monday()))
    .rem_euclid(7);
    candidate + Duration::days(days_ahead)
}

/// Where a pot is in its life. Closed, because every read that branches on it
/// has to say what a rolled pot means as well as a drawn one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PotStatus {
    /// Taking tickets. Exactly one pot is in this state at a time.
    Open,
    /// Drawn with tickets in it: it has a winner and paid out.
    Drawn,
    /// The draw hour came and nobody had bought in. Nothing was paid, and the
    /// next pot opened behind it.
    Rolled,
}

impl PotStatus {
    /// The persisted `pots.status` value.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Drawn => "drawn",
            Self::Rolled => "rolled",
        }
    }

    /// A status the database wrote. A value outside the CHECK constraint is
    /// impossible, so this crashes rather than inventing a state.
    pub fn from_db(value: &str) -> Self {
        match value {
            "open" => Self::Open,
            "drawn" => Self::Drawn,
            "rolled" => Self::Rolled,
            other => panic!("unknown pot status in the database: {other}"),
        }
    }
}

/// What rides the notify. Everything a replica other than the acting one
/// needs in order to tell the winner they won, wherever they are connected;
/// the size and the countdown come from re-reading the table.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PotChange {
    /// Someone bought in: every replica re-reads the size.
    Bought,
    /// A draw settled with a winner.
    Drawn {
        winner_user_id: Uuid,
        payout_chips: i64,
        winner_tickets: i64,
        total_tickets: i64,
    },
    /// The draw hour came with nobody in the pot. Nothing was paid; a fresh
    /// pot is open.
    Rolled,
}

impl PotChange {
    /// Parse a notify payload. A payload this cannot read is a bug on the
    /// sending side, not a reason to skip the re-read, so the caller logs it
    /// and refreshes anyway.
    pub fn parse(payload: &str) -> Result<Self> {
        serde_json::from_str(payload).context("parsing pot_changed payload")
    }
}

pub async fn listen_for_pot_changes(client: &Client) -> Result<()> {
    client
        .batch_execute(&format!("LISTEN {POT_CHANGED_CHANNEL};"))
        .await?;
    Ok(())
}

/// One pot. The settled fields are all `None` while it is open, and the
/// table's CHECK constraints are what keep a half-settled row from existing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pot {
    pub id: Uuid,
    pub opens_at: DateTime<Utc>,
    pub draws_at: DateTime<Utc>,
    pub status: PotStatus,
    pub ticket_price: i64,
    /// Set at the draw; `None` on an open pot, and on a settled one whose
    /// winner has since deleted their account.
    pub winner_user_id: Option<Uuid>,
    /// The settled record of what was in it. `None` while open: a live pot's
    /// size is its tickets, never a stored total.
    pub ticket_count: Option<i64>,
    pub payout_chips: Option<i64>,
    pub drawn_at: Option<DateTime<Utc>>,
}

impl From<Row> for Pot {
    fn from(row: Row) -> Self {
        Self {
            id: row.get("id"),
            opens_at: row.get("opens_at"),
            draws_at: row.get("draws_at"),
            status: PotStatus::from_db(row.get("status")),
            ticket_price: row.get("ticket_price"),
            winner_user_id: row.get("winner_user_id"),
            ticket_count: row.get("ticket_count"),
            payout_chips: row.get("payout_chips"),
            drawn_at: row.get("drawn_at"),
        }
    }
}

/// One player's holding in one pot: the shape the draw walks and the
/// snapshot indexes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PotTicketHolder {
    pub user_id: Uuid,
    /// The whole holding, what the draw weighs.
    pub tickets: i64,
    /// The part bought today (UTC), what the daily cap counts. Read at
    /// query time, so a snapshot straddling midnight is stale until the next
    /// refresh (the sweeper's minute at most).
    pub bought_today: i64,
}

/// A settled draw, computed before anything is written. Every number the
/// payout, the ledger, and the #lounge line need is here, so the transaction
/// below writes what this says and invents nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PotDraw {
    pub winner_user_id: Uuid,
    pub winner_tickets: i64,
    pub total_tickets: i64,
    /// What the tickets paid in: `total_tickets * ticket_price`.
    pub size: i64,
    pub payout_chips: i64,
}

/// Pull one ticket, weighted by holding. Pure and seedable, so the draw is a
/// function of (holders, seed) and a test can assert the whole result.
///
/// `None` means nobody bought in, which is the rolled case: there is no
/// winner to invent.
pub fn draw_from_seed(
    holders: &[PotTicketHolder],
    ticket_price: i64,
    seed: u64,
) -> Option<PotDraw> {
    let total_tickets: i64 = holders.iter().map(|holder| holder.tickets).sum();
    if total_tickets <= 0 {
        return None;
    }
    // One xorshift step, so a seed taken from the wall clock (whose low bits
    // move in lockstep with the sweeper's interval) does not bias the walk.
    let roll = mix(seed) % (total_tickets as u64);
    let mut walked: u64 = 0;
    for holder in holders {
        walked += holder.tickets as u64;
        if roll < walked {
            let size = total_tickets.saturating_mul(ticket_price);
            return Some(PotDraw {
                winner_user_id: holder.user_id,
                winner_tickets: holder.tickets,
                total_tickets,
                size,
                payout_chips: payout_for(size),
            });
        }
    }
    // `roll` is strictly below the sum the walk accumulates, so the loop
    // always returns. Reaching here would mean the sum changed mid-walk.
    unreachable!("the weighted walk covers every ticket")
}

/// xorshift64: one round is plenty to decorrelate a timestamp seed, and it
/// keeps the draw a pure function of the seed with no dependency.
fn mix(seed: u64) -> u64 {
    let mut x = match seed {
        0 => 0xA409_3822_299F_31D0,
        seed => seed,
    };
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    x
}

impl Pot {
    /// The pot taking tickets right now, if there is one.
    pub async fn find_open(client: &impl GenericClient) -> Result<Option<Self>> {
        let row = client
            .query_opt("SELECT * FROM pots WHERE status = 'open'", &[])
            .await?;
        Ok(row.map(Self::from))
    }

    /// Serialize every pot mutation against every other one.
    ///
    /// The advisory lock is what makes opening a pot exact: when none is
    /// open there is no row to take `FOR UPDATE`, so without it two replicas
    /// sweeping at the same second would both insert and one would die on the
    /// partial unique index. Same reasoning as `CrownReign::lock_open`.
    pub async fn lock_open(tx: &Transaction<'_>) -> Result<Option<Self>> {
        tx.query_one(
            "SELECT pg_advisory_xact_lock(hashtextextended($1, 0))",
            &[&POT_CHANGED_CHANNEL],
        )
        .await?;
        let row = tx
            .query_opt("SELECT * FROM pots WHERE status = 'open' FOR UPDATE", &[])
            .await?;
        Ok(row.map(Self::from))
    }

    /// Open a pot that draws at `draws_at`. The caller decides the hour (the
    /// service from [`next_draw_at`], a test from whatever it needs), because
    /// a default here would silently pick when money moves.
    pub async fn open_in_tx(
        tx: &Transaction<'_>,
        draws_at: DateTime<Utc>,
        ticket_price: i64,
    ) -> Result<Self> {
        let row = tx
            .query_one(
                "INSERT INTO pots (draws_at, ticket_price) VALUES ($1, $2) RETURNING *",
                &[&draws_at, &ticket_price],
            )
            .await?;
        Ok(Self::from(row))
    }

    /// Lock the open pot for a buy: the row lock is what makes the per-user
    /// cap exact (two concurrent buys by one player serialize here rather
    /// than both passing the cap check) and what keeps a buy from landing in
    /// a pot the sweeper is drawing.
    pub async fn lock_open_for_buy(tx: &Transaction<'_>) -> Result<Option<Self>> {
        let row = tx
            .query_opt("SELECT * FROM pots WHERE status = 'open' FOR UPDATE", &[])
            .await?;
        Ok(row.map(Self::from))
    }

    /// Settle a pot that had tickets in it. Guarded on `status = 'open'`, so
    /// two replicas sweeping the same pot produce exactly one payout: the
    /// loser gets `None` and rolls its transaction back.
    pub async fn settle_drawn_in_tx(
        tx: &Transaction<'_>,
        pot_id: Uuid,
        draw: &PotDraw,
    ) -> Result<Option<Self>> {
        let row = tx
            .query_opt(
                "UPDATE pots
                 SET status = 'drawn',
                     winner_user_id = $2,
                     ticket_count = $3,
                     payout_chips = $4,
                     drawn_at = current_timestamp
                 WHERE id = $1 AND status = 'open'
                 RETURNING *",
                &[
                    &pot_id,
                    &draw.winner_user_id,
                    &draw.total_tickets,
                    &draw.payout_chips,
                ],
            )
            .await?;
        Ok(row.map(Self::from))
    }

    /// Settle a pot nobody bought into. Same status guard, no payout.
    pub async fn settle_rolled_in_tx(tx: &Transaction<'_>, pot_id: Uuid) -> Result<Option<Self>> {
        let row = tx
            .query_opt(
                "UPDATE pots
                 SET status = 'rolled',
                     ticket_count = 0,
                     payout_chips = 0,
                     drawn_at = current_timestamp
                 WHERE id = $1 AND status = 'open'
                 RETURNING *",
                &[&pot_id],
            )
            .await?;
        Ok(row.map(Self::from))
    }

    /// Tell every replica the pot moved. Sent inside the acting transaction,
    /// so Postgres delivers it on commit and never for a rolled-back buy.
    pub async fn notify_changed(tx: &Transaction<'_>, change: &PotChange) -> Result<()> {
        let payload = serde_json::to_string(change).context("encoding pot_changed payload")?;
        tx.execute(
            "SELECT pg_notify($1, $2)",
            &[&POT_CHANGED_CHANNEL, &payload],
        )
        .await?;
        Ok(())
    }
}

/// The tickets themselves. One row per buy, never updated.
pub struct PotTicket;

impl PotTicket {
    /// Buy `count` tickets, refusing in the query when the buyer is already
    /// at today's cap. `None` means the cap said no; the caller turns that
    /// into a refusal and rolls back, so nothing was charged. `Some` is the
    /// buyer's whole holding in the pot after the buy.
    ///
    /// The cap is per UTC day (`created`, in UTC, on today's date), enforced
    /// by the `WHERE` on the insert, and made exact by the caller holding
    /// [`Pot::lock_open_for_buy`]: without that lock two concurrent buys by
    /// one player could both read the same sum.
    pub async fn buy_in_tx(
        tx: &Transaction<'_>,
        pot_id: Uuid,
        user_id: Uuid,
        count: i64,
        daily_cap: i64,
    ) -> Result<Option<i64>> {
        let inserted = tx
            .query_opt(
                "INSERT INTO pot_tickets (pot_id, user_id, count)
                 SELECT $1, $2, $3
                 WHERE COALESCE(
                     (SELECT SUM(count) FROM pot_tickets
                      WHERE pot_id = $1 AND user_id = $2
                        AND (created AT TIME ZONE 'UTC')::date
                            = (current_timestamp AT TIME ZONE 'UTC')::date),
                     0
                 )::BIGINT + $3 <= $4
                 RETURNING id",
                &[&pot_id, &user_id, &count, &daily_cap],
            )
            .await?;
        match inserted {
            None => Ok(None),
            // Read back inside the same transaction, so the total includes
            // the row just written.
            Some(_) => Self::user_total(tx, pot_id, user_id).await.map(Some),
        }
    }

    /// How many tickets one player holds in one pot. Scoped to the user in
    /// the query, so no caller can widen it by accident.
    pub async fn user_total(
        client: &impl GenericClient,
        pot_id: Uuid,
        user_id: Uuid,
    ) -> Result<i64> {
        let row = client
            .query_one(
                "SELECT COALESCE(SUM(count), 0)::BIGINT AS tickets
                 FROM pot_tickets
                 WHERE pot_id = $1 AND user_id = $2",
                &[&pot_id, &user_id],
            )
            .await?;
        Ok(row.get("tickets"))
    }

    /// How many tickets one player bought in one pot today (UTC), the number
    /// the daily cap counts. Scoped to the user in the query.
    pub async fn user_total_today(
        client: &impl GenericClient,
        pot_id: Uuid,
        user_id: Uuid,
    ) -> Result<i64> {
        let row = client
            .query_one(
                "SELECT COALESCE(SUM(count), 0)::BIGINT AS tickets
                 FROM pot_tickets
                 WHERE pot_id = $1 AND user_id = $2
                   AND (created AT TIME ZONE 'UTC')::date
                       = (current_timestamp AT TIME ZONE 'UTC')::date",
                &[&pot_id, &user_id],
            )
            .await?;
        Ok(row.get("tickets"))
    }

    /// Every holder in a pot, ordered by user id so the seeded draw is a
    /// function of the tickets alone and not of the planner's mood. Carries
    /// today's part of each holding too, so the snapshot can answer "how many
    /// more today" without a second query.
    pub async fn holders(
        client: &impl GenericClient,
        pot_id: Uuid,
    ) -> Result<Vec<PotTicketHolder>> {
        let rows = client
            .query(
                "SELECT user_id,
                        SUM(count)::BIGINT AS tickets,
                        COALESCE(SUM(count) FILTER (
                            WHERE (created AT TIME ZONE 'UTC')::date
                                = (current_timestamp AT TIME ZONE 'UTC')::date
                        ), 0)::BIGINT AS bought_today
                 FROM pot_tickets
                 WHERE pot_id = $1
                 GROUP BY user_id
                 ORDER BY user_id",
                &[&pot_id],
            )
            .await?;
        Ok(rows
            .into_iter()
            .map(|row| PotTicketHolder {
                user_id: row.get("user_id"),
                tickets: row.get("tickets"),
                bought_today: row.get("bought_today"),
            })
            .collect())
    }
}
