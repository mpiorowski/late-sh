//! The ring (`app/deadchannel/ring`, GAME.md "The arena pass"): a live 1v1
//! card duel between two runners that the rest of the street bets bits
//! on. This module owns every read and write of `deadchannel_bouts` (the
//! calls and the bouts), `deadchannel_bout_bets` (the bets on them) and
//! `deadchannel_ring_decks` (the deck each runner brings), and the channel
//! a bout's every change travels on.
//!
//! A bout is truth: who fought, who won and who bet decide who is paid, so
//! it is a row, and every change to it is one conditional statement that
//! says whether it landed, so the caller knows whether it won; a stored
//! round, decided under the row lock (`lock`), is the one exception. The
//! three unique indexes of migration 287 are the ring's rules (one bout in
//! the ring, one open call per caller, a pair once a day), so a write that
//! loses one is the rule refusing, not a bug. The duel itself is `state`,
//! JSON this module stores and never reads.

use std::collections::HashMap;

use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio_postgres::error::SqlState;
use tokio_postgres::{GenericClient, Row};
use uuid::Uuid;

/// Cross-process channel for every change to a bout or its bets, fired
/// by the tables' own triggers (migration 287). The payload is the bout's
/// id; the listener re-reads the whole board.
pub const DEADCHANNEL_BOUT_CHANNEL: &str = "deadchannel_bout";

/// The unique index that keeps one bout in the ring.
const ONE_RING: &str = "deadchannel_bouts_one_ring";
/// The unique index that keeps a pair to one bout a day.
const ONE_PER_PAIR: &str = "deadchannel_bouts_one_per_pair";

/// A corner of the ring.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    /// The runner who made the call.
    Caller,
    /// The runner who was called out.
    Called,
}

impl Side {
    pub fn as_str(self) -> &'static str {
        match self {
            Side::Caller => "caller",
            Side::Called => "called",
        }
    }

    fn parse(value: &str) -> Result<Side> {
        match value {
            "caller" => Ok(Side::Caller),
            "called" => Ok(Side::Called),
            other => anyhow::bail!("unknown bout side {other:?}"),
        }
    }

    /// The corner across the ring.
    pub fn other(self) -> Side {
        match self {
            Side::Caller => Side::Called,
            Side::Called => Side::Caller,
        }
    }

    fn pool_column(self) -> &'static str {
        match self {
            Side::Caller => "pool_caller",
            Side::Called => "pool_called",
        }
    }

    fn settled_column(self) -> &'static str {
        match self {
            Side::Caller => "caller_settled_at",
            Side::Called => "called_settled_at",
        }
    }
}

/// How a bout ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoutOutcome {
    /// The caller won the fight.
    Caller,
    /// The called runner won the fight.
    Called,
    /// The fight ended level.
    Draw,
    /// The call ended unaccepted: turned down, taken back by the caller,
    /// or refused at the accept.
    Declined,
    /// Nobody answered the call before it lapsed.
    Expired,
}

impl BoutOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            BoutOutcome::Caller => "caller",
            BoutOutcome::Called => "called",
            BoutOutcome::Draw => "draw",
            BoutOutcome::Declined => "declined",
            BoutOutcome::Expired => "expired",
        }
    }

    fn parse(value: &str) -> Result<BoutOutcome> {
        match value {
            "caller" => Ok(BoutOutcome::Caller),
            "called" => Ok(BoutOutcome::Called),
            "draw" => Ok(BoutOutcome::Draw),
            "declined" => Ok(BoutOutcome::Declined),
            "expired" => Ok(BoutOutcome::Expired),
            other => anyhow::bail!("unknown bout outcome {other:?}"),
        }
    }
}

/// One call to the ring, and the bout it became if it was accepted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bout {
    pub id: Uuid,
    /// The UTC day of the call: a pair fights once on it.
    pub day: NaiveDate,
    pub caller_user_id: Uuid,
    pub called_user_id: Uuid,
    pub caller_username: String,
    pub called_username: String,
    /// The decks the fighters brought, snapshotted at the accept; `None`
    /// while the call is open.
    pub caller_deck: Option<Value>,
    pub called_deck: Option<Value>,
    pub called_at: DateTime<Utc>,
    /// When an unanswered call lapses.
    pub expires_at: DateTime<Utc>,
    pub accepted_at: Option<DateTime<Utc>>,
    /// When the fight starts and the bets close; set with `accepted_at`.
    pub bell_at: Option<DateTime<Utc>>,
    /// When the current round is forced.
    pub deadline_at: Option<DateTime<Utc>>,
    /// The duel, as the app wrote it.
    pub state: Option<Value>,
    /// Bits bet on the caller.
    pub pool_caller: i64,
    /// Bits bet on the called runner.
    pub pool_called: i64,
    pub ended_at: Option<DateTime<Utc>>,
    pub outcome: Option<BoutOutcome>,
    /// The caller's settle claim, taken once after a fought bout ends.
    pub caller_settled_at: Option<DateTime<Utc>>,
    /// The called runner's settle claim, same terms.
    pub called_settled_at: Option<DateTime<Utc>>,
}

/// What a new call says. The id is minted on insert.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewCall {
    pub day: NaiveDate,
    pub caller_user_id: Uuid,
    pub called_user_id: Uuid,
    pub caller_username: String,
    pub called_username: String,
    pub called_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

/// What a call came back with. The bout is boxed: it dwarfs the
/// refusals.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CallResult {
    /// The call is on the row, open.
    Called(Box<Bout>),
    /// A bout is being fought: nobody calls until it ends.
    RingTaken,
    /// The caller already has a call or a bout open.
    AlreadyCalling,
    /// The caller's last call to this runner has not run out its time,
    /// however it ended: a call taken back is not a way to ring the same
    /// runner over and over.
    TooSoon,
}

/// What an accept came back with. The bout is boxed: it dwarfs the
/// refusals.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AcceptResult {
    /// The bout is on: decks in, round 1, the bell set.
    Accepted(Box<Bout>),
    /// No open call under that id: accepted, declined or lapsed already.
    Gone,
    /// Another bout took the ring first.
    RingTaken,
    /// The pair has already fought on the call's day.
    FoughtToday,
}

impl Bout {
    fn from_row(row: &Row) -> Result<Self> {
        let outcome: Option<String> = row.get("outcome");
        Ok(Self {
            id: row.get("id"),
            day: row.get("day"),
            caller_user_id: row.get("caller_user_id"),
            called_user_id: row.get("called_user_id"),
            caller_username: row.get("caller_username"),
            called_username: row.get("called_username"),
            caller_deck: row.get("caller_deck"),
            called_deck: row.get("called_deck"),
            called_at: row.get("called_at"),
            expires_at: row.get("expires_at"),
            accepted_at: row.get("accepted_at"),
            bell_at: row.get("bell_at"),
            deadline_at: row.get("deadline_at"),
            state: row.get("state"),
            pool_caller: row.get("pool_caller"),
            pool_called: row.get("pool_called"),
            ended_at: row.get("ended_at"),
            outcome: outcome.as_deref().map(BoutOutcome::parse).transpose()?,
            caller_settled_at: row.get("caller_settled_at"),
            called_settled_at: row.get("called_settled_at"),
        })
    }

    pub fn user(&self, side: Side) -> Uuid {
        match side {
            Side::Caller => self.caller_user_id,
            Side::Called => self.called_user_id,
        }
    }

    pub fn username(&self, side: Side) -> &str {
        match side {
            Side::Caller => &self.caller_username,
            Side::Called => &self.called_username,
        }
    }

    /// The corner `user_id` fights in; `None` for anyone else.
    pub fn side_of(&self, user_id: Uuid) -> Option<Side> {
        match user_id {
            id if id == self.caller_user_id => Some(Side::Caller),
            id if id == self.called_user_id => Some(Side::Called),
            _ => None,
        }
    }

    /// Whether the call became a bout.
    pub fn is_accepted(&self) -> bool {
        self.accepted_at.is_some()
    }

    /// Put a call on the row. Refused while a bout is being fought (the
    /// insert finds one), while the caller has a call or a bout open (the
    /// one-call index), or while an earlier call of theirs to the same
    /// runner would still be waiting had it not ended (its `expires_at`
    /// is ahead of this call's `called_at`); when nothing went in, a
    /// second read tells the three apart.
    pub async fn call(client: &impl GenericClient, new: &NewCall) -> Result<CallResult> {
        let row = client
            .query_opt(
                "INSERT INTO deadchannel_bouts
                     (id, day, caller_user_id, called_user_id, caller_username,
                      called_username, called_at, expires_at)
                 SELECT $1, $2, $3, $4, $5, $6, $7, $8
                 WHERE NOT EXISTS (
                     SELECT 1 FROM deadchannel_bouts
                     WHERE accepted_at IS NOT NULL AND ended_at IS NULL
                 )
                 AND NOT EXISTS (
                     SELECT 1 FROM deadchannel_bouts
                     WHERE caller_user_id = $3 AND called_user_id = $4
                       AND expires_at > $7
                 )
                 ON CONFLICT (caller_user_id) WHERE ended_at IS NULL DO NOTHING
                 RETURNING *",
                &[
                    &Uuid::now_v7(),
                    &new.day,
                    &new.caller_user_id,
                    &new.called_user_id,
                    &new.caller_username,
                    &new.called_username,
                    &new.called_at,
                    &new.expires_at,
                ],
            )
            .await
            .context("calling a runner to the ring")?;
        match row {
            Some(row) => Ok(CallResult::Called(Box::new(Self::from_row(&row)?))),
            None => Self::call_refusal(client, new.caller_user_id).await,
        }
    }

    /// Why a call went nowhere: the caller's own open row if they have
    /// one, the bout in the ring if there is one, and otherwise the call
    /// they made to the same runner a moment ago.
    async fn call_refusal(client: &impl GenericClient, caller_user_id: Uuid) -> Result<CallResult> {
        let row = client
            .query_one(
                "SELECT EXISTS (
                     SELECT 1 FROM deadchannel_bouts
                     WHERE caller_user_id = $1 AND ended_at IS NULL
                 ) AS calling,
                 EXISTS (
                     SELECT 1 FROM deadchannel_bouts
                     WHERE accepted_at IS NOT NULL AND ended_at IS NULL
                 ) AS taken",
                &[&caller_user_id],
            )
            .await
            .context("reading why the call went nowhere")?;
        match (row.get("calling"), row.get("taken")) {
            (true, _) => Ok(CallResult::AlreadyCalling),
            (false, true) => Ok(CallResult::RingTaken),
            (false, false) => Ok(CallResult::TooSoon),
        }
    }

    /// Whether the ring still holds something of this runner's: a bet it
    /// has not paid, or a fought bout that has not settled them. The
    /// ledge asks before it lets a runner go, so nothing staked before
    /// the fall is paid onto the sheet after it.
    pub async fn holds(client: &impl GenericClient, user_id: Uuid) -> Result<bool> {
        let row = client
            .query_one(
                "SELECT EXISTS (
                     SELECT 1 FROM deadchannel_bout_bets
                     WHERE user_id = $1 AND paid_at IS NULL
                 ) OR EXISTS (
                     SELECT 1 FROM deadchannel_bouts
                     WHERE accepted_at IS NOT NULL
                       AND ((caller_user_id = $1 AND caller_settled_at IS NULL)
                            OR (called_user_id = $1 AND called_settled_at IS NULL))
                 ) AS held",
                &[&user_id],
            )
            .await
            .context("reading what the ring holds of the runner")?;
        Ok(row.get("held"))
    }

    /// The row, locked for the caller's transaction: every change a round
    /// makes is decided under this lock.
    pub async fn lock(client: &impl GenericClient, id: Uuid) -> Result<Option<Bout>> {
        let row = client
            .query_opt(
                "SELECT * FROM deadchannel_bouts WHERE id = $1 FOR UPDATE",
                &[&id],
            )
            .await
            .context("locking the bout")?;
        row.as_ref().map(Self::from_row).transpose()
    }

    pub async fn get(client: &impl GenericClient, id: Uuid) -> Result<Option<Bout>> {
        let row = client
            .query_opt("SELECT * FROM deadchannel_bouts WHERE id = $1", &[&id])
            .await
            .context("reading the bout")?;
        row.as_ref().map(Self::from_row).transpose()
    }

    /// Accept an open call: snapshot both decks, store the duel, start
    /// round 1 due by `deadline_at` and ring the bell at `bell_at`. One
    /// conditional update on a call still open and not past its time at
    /// `now`; `Gone` when there is none.
    ///
    /// Accepting is what claims the ring and the pair's day, through the
    /// unique indexes `deadchannel_bouts_one_ring` and
    /// `deadchannel_bouts_one_per_pair`, so losing either one is a unique
    /// violation, mapped to `RingTaken` or `FoughtToday`; any other error
    /// propagates. A failed statement aborts the transaction it runs in,
    /// so the update runs in a nested transaction of its own:
    /// `GenericClient::transaction` opens a `SAVEPOINT` on a transaction
    /// and a `BEGIN` on a plain client, and a refusal rolls back only
    /// that. Opening one needs `&mut`, which is why `accept` alone takes
    /// `&mut impl GenericClient`: pass `&mut *tx` from inside the caller's
    /// transaction, which stays usable after a refusal, or a plain client.
    #[allow(clippy::too_many_arguments)]
    pub async fn accept(
        client: &mut impl GenericClient,
        id: Uuid,
        caller_deck: &Value,
        called_deck: &Value,
        state: &Value,
        bell_at: DateTime<Utc>,
        deadline_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<AcceptResult> {
        let savepoint = client
            .transaction()
            .await
            .context("opening the accept's savepoint")?;
        let updated = savepoint
            .query_opt(
                "UPDATE deadchannel_bouts
                 SET accepted_at = $2, bell_at = $3, deadline_at = $4,
                     caller_deck = $5, called_deck = $6, state = $7
                 WHERE id = $1 AND accepted_at IS NULL AND ended_at IS NULL
                   AND expires_at > $2
                 RETURNING *",
                &[
                    &id,
                    &now,
                    &bell_at,
                    &deadline_at,
                    caller_deck,
                    called_deck,
                    state,
                ],
            )
            .await;
        match updated {
            Ok(Some(row)) => {
                let bout = Self::from_row(&row)?;
                savepoint
                    .commit()
                    .await
                    .context("releasing the accept's savepoint")?;
                Ok(AcceptResult::Accepted(Box::new(bout)))
            }
            Ok(None) => {
                savepoint
                    .commit()
                    .await
                    .context("releasing the accept's savepoint")?;
                Ok(AcceptResult::Gone)
            }
            Err(error) => match ring_refusal(&error) {
                Some(refusal) => {
                    savepoint
                        .rollback()
                        .await
                        .context("rolling back the refused accept")?;
                    Ok(refusal)
                }
                None => Err(anyhow::Error::new(error).context("accepting the bout")),
            },
        }
    }

    /// End an open call that was never accepted, as declined or expired.
    /// `Some` for the one caller that ended it.
    pub async fn end_call(
        client: &impl GenericClient,
        id: Uuid,
        outcome: BoutOutcome,
        now: DateTime<Utc>,
    ) -> Result<Option<Bout>> {
        let row = client
            .query_opt(
                "UPDATE deadchannel_bouts
                 SET ended_at = $2, outcome = $3
                 WHERE id = $1 AND accepted_at IS NULL AND ended_at IS NULL
                 RETURNING *",
                &[&id, &now, &outcome.as_str()],
            )
            .await
            .context("ending the call")?;
        row.as_ref().map(Self::from_row).transpose()
    }

    /// End every unanswered call at or past its time as expired. Returns
    /// the ones this call ended: whoever ends one says so on the wire.
    pub async fn expire_calls(
        client: &impl GenericClient,
        now: DateTime<Utc>,
    ) -> Result<Vec<Bout>> {
        let rows = client
            .query(
                "UPDATE deadchannel_bouts
                 SET ended_at = $1, outcome = $2
                 WHERE accepted_at IS NULL AND ended_at IS NULL AND expires_at <= $1
                 RETURNING *",
                &[&now, &BoutOutcome::Expired.as_str()],
            )
            .await
            .context("expiring the lapsed calls")?;
        rows.iter().map(Self::from_row).collect()
    }

    /// Store a resolved round. Unconditional: the caller holds the row
    /// lock (`lock`) and decided the round under it.
    pub async fn store_round(
        client: &impl GenericClient,
        id: Uuid,
        deadline_at: Option<DateTime<Utc>>,
        state: &Value,
    ) -> Result<()> {
        let updated = client
            .execute(
                "UPDATE deadchannel_bouts
                 SET deadline_at = $2, state = $3
                 WHERE id = $1",
                &[&id, &deadline_at, state],
            )
            .await
            .context("storing the bout's round")?;
        match updated {
            1 => Ok(()),
            other => anyhow::bail!("storing a round of bout {id} updated {other} rows"),
        }
    }

    /// End a fought bout, once: conditional on it being accepted and still
    /// open. `Some` for the one caller that ended it.
    pub async fn end(
        client: &impl GenericClient,
        id: Uuid,
        outcome: BoutOutcome,
        state: &Value,
        now: DateTime<Utc>,
    ) -> Result<Option<Bout>> {
        let row = client
            .query_opt(
                "UPDATE deadchannel_bouts
                 SET ended_at = $2, outcome = $3, state = $4
                 WHERE id = $1 AND accepted_at IS NOT NULL AND ended_at IS NULL
                 RETURNING *",
                &[&id, &now, &outcome.as_str(), state],
            )
            .await
            .context("ending the bout")?;
        row.as_ref().map(Self::from_row).transpose()
    }

    /// Add `bits` to `side`'s pool, only while the bout is accepted, open
    /// and before its bell at `now`. Returns whether it applied.
    pub async fn add_to_pool(
        client: &impl GenericClient,
        id: Uuid,
        side: Side,
        bits: i64,
        now: DateTime<Utc>,
    ) -> Result<bool> {
        let pool = side.pool_column();
        let statement = format!(
            "UPDATE deadchannel_bouts
             SET {pool} = {pool} + $2
             WHERE id = $1 AND accepted_at IS NOT NULL AND ended_at IS NULL
               AND bell_at > $3"
        );
        let updated = client
            .execute(&statement, &[&id, &bits, &now])
            .await
            .context("adding to the bout's pool")?;
        Ok(updated == 1)
    }

    /// Take `side`'s settle claim on a fought bout that ended. `true` for
    /// the one caller that gets to settle that fighter; `false` for
    /// everyone after, and before the bout ends.
    pub async fn claim_fighter_settle(
        client: &impl GenericClient,
        id: Uuid,
        side: Side,
        now: DateTime<Utc>,
    ) -> Result<bool> {
        let settled = side.settled_column();
        let statement = format!(
            "UPDATE deadchannel_bouts
             SET {settled} = $2
             WHERE id = $1 AND {settled} IS NULL
               AND accepted_at IS NOT NULL AND ended_at IS NOT NULL"
        );
        let updated = client
            .execute(&statement, &[&id, &now])
            .await
            .context("claiming the fighter's settle")?;
        Ok(updated == 1)
    }

    /// What the ring's board shows: every open call and bout, and the
    /// fought bouts that ended at or after `since`. Oldest call first.
    pub async fn board(client: &impl GenericClient, since: DateTime<Utc>) -> Result<Vec<Bout>> {
        let rows = client
            .query(
                "SELECT * FROM deadchannel_bouts
                 WHERE ended_at IS NULL
                    OR (accepted_at IS NOT NULL AND ended_at >= $1)
                 ORDER BY called_at, id",
                &[&since],
            )
            .await
            .context("reading the ring's board")?;
        rows.iter().map(Self::from_row).collect()
    }

    /// Fought bouts that ended with a fighter's settle claim untaken or a
    /// bet unpaid: what a replica that died mid-settle left behind, for
    /// any sweeper to finish. Oldest ending first.
    pub async fn unsettled(client: &impl GenericClient) -> Result<Vec<Bout>> {
        let rows = client
            .query(
                "SELECT * FROM deadchannel_bouts AS bout
                 WHERE bout.accepted_at IS NOT NULL AND bout.ended_at IS NOT NULL
                   AND (bout.caller_settled_at IS NULL
                        OR bout.called_settled_at IS NULL
                        OR EXISTS (
                            SELECT 1 FROM deadchannel_bout_bets AS bet
                            WHERE bet.bout_id = bout.id AND bet.paid_at IS NULL
                        ))
                 ORDER BY bout.ended_at, bout.id",
                &[],
            )
            .await
            .context("listing the unsettled bouts")?;
        rows.iter().map(Self::from_row).collect()
    }

    /// Every runner's ring record over the fought bouts that ended, as
    /// (wins, losses). A draw counts for neither; a runner with no win or
    /// loss is absent.
    pub async fn records(client: &impl GenericClient) -> Result<HashMap<Uuid, (i32, i32)>> {
        let rows = client
            .query(
                "SELECT user_id,
                        (count(*) FILTER (WHERE won))::int AS wins,
                        (count(*) FILTER (WHERE NOT won))::int AS losses
                 FROM (
                     SELECT caller_user_id AS user_id, outcome = 'caller' AS won
                     FROM deadchannel_bouts
                     WHERE accepted_at IS NOT NULL AND outcome IN ('caller', 'called')
                     UNION ALL
                     SELECT called_user_id AS user_id, outcome = 'called' AS won
                     FROM deadchannel_bouts
                     WHERE accepted_at IS NOT NULL AND outcome IN ('caller', 'called')
                 ) AS fights
                 GROUP BY user_id",
                &[],
            )
            .await
            .context("reading the ring records")?;
        Ok(rows
            .iter()
            .map(|row| (row.get("user_id"), (row.get("wins"), row.get("losses"))))
            .collect())
    }
}

/// The refusal a unique violation on `accept`'s update stands for; `None`
/// for any other error.
fn ring_refusal(error: &tokio_postgres::Error) -> Option<AcceptResult> {
    match error.as_db_error() {
        Some(db_error) if db_error.code() == &SqlState::UNIQUE_VIOLATION => {
            match db_error.constraint() {
                Some(ONE_RING) => Some(AcceptResult::RingTaken),
                Some(ONE_PER_PAIR) => Some(AcceptResult::FoughtToday),
                _ => None,
            }
        }
        _ => None,
    }
}

/// One runner's bet on a bout: one side, pressed again to add bits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoutBet {
    pub bout_id: Uuid,
    pub user_id: Uuid,
    pub username: String,
    pub side: Side,
    /// Every bit on the side, all presses summed.
    pub bits: i64,
    /// How many bets are folded into the row.
    pub presses: i32,
    /// When the first press landed.
    pub placed_at: DateTime<Utc>,
    /// The pay claim; `None` until it is taken.
    pub paid_at: Option<DateTime<Utc>>,
    /// What the pay claim paid; `None` until it is taken.
    pub paid_bits: Option<i64>,
}

/// What a stake came back with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StakeResult {
    /// The bits are on the row; the bet after this press.
    Staked(BoutBet),
    /// This runner already bet on the other side.
    OtherSide,
    /// This runner has pressed as many times as a bet allows.
    Capped,
}

impl BoutBet {
    fn from_row(row: &Row) -> Result<Self> {
        let side: String = row.get("side");
        Ok(Self {
            bout_id: row.get("bout_id"),
            user_id: row.get("user_id"),
            username: row.get("username"),
            side: Side::parse(&side)?,
            bits: row.get("bits"),
            presses: row.get("presses"),
            placed_at: row.get("placed_at"),
            paid_at: row.get("paid_at"),
            paid_bits: row.get("paid_bits"),
        })
    }

    /// Put `bits` on `side`: a new bet, or one more press on this
    /// runner's bet, adding the bits, while it is on the same side and
    /// under `max_presses`. When nothing came back, a read of the bet
    /// tells the other side from the cap. Whether the bout still takes
    /// bets is `Bout::add_to_pool`'s to say, in the same transaction.
    #[allow(clippy::too_many_arguments)]
    pub async fn stake(
        client: &impl GenericClient,
        bout_id: Uuid,
        user_id: Uuid,
        username: &str,
        side: Side,
        bits: i64,
        max_presses: i32,
        now: DateTime<Utc>,
    ) -> Result<StakeResult> {
        let row = client
            .query_opt(
                "INSERT INTO deadchannel_bout_bets
                     (bout_id, user_id, username, side, bits, placed_at)
                 VALUES ($1, $2, $3, $4, $5, $6)
                 ON CONFLICT (bout_id, user_id) DO UPDATE
                 SET bits = deadchannel_bout_bets.bits + EXCLUDED.bits,
                     presses = deadchannel_bout_bets.presses + 1
                 WHERE deadchannel_bout_bets.side = EXCLUDED.side
                   AND deadchannel_bout_bets.presses < $7
                 RETURNING *",
                &[
                    &bout_id,
                    &user_id,
                    &username,
                    &side.as_str(),
                    &bits,
                    &now,
                    &max_presses,
                ],
            )
            .await
            .context("staking bits on the bout")?;
        match row {
            Some(row) => Ok(StakeResult::Staked(Self::from_row(&row)?)),
            None => match Self::get(client, bout_id, user_id).await? {
                Some(bet) if bet.side != side => Ok(StakeResult::OtherSide),
                Some(_) => Ok(StakeResult::Capped),
                None => {
                    anyhow::bail!("the bet on bout {bout_id} refused a stake and then vanished")
                }
            },
        }
    }

    async fn get(
        client: &impl GenericClient,
        bout_id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<BoutBet>> {
        let row = client
            .query_opt(
                "SELECT * FROM deadchannel_bout_bets WHERE bout_id = $1 AND user_id = $2",
                &[&bout_id, &user_id],
            )
            .await
            .context("reading the bet")?;
        row.as_ref().map(Self::from_row).transpose()
    }

    /// Every bet on a bout, first placed first.
    pub async fn list(client: &impl GenericClient, bout_id: Uuid) -> Result<Vec<BoutBet>> {
        let rows = client
            .query(
                "SELECT * FROM deadchannel_bout_bets
                 WHERE bout_id = $1
                 ORDER BY placed_at, user_id",
                &[&bout_id],
            )
            .await
            .context("listing the bout's bets")?;
        rows.iter().map(Self::from_row).collect()
    }

    /// Take a bet's pay claim, recording `paid_bits`. `Some` for the one
    /// caller that gets to pay it; `None` for everyone after.
    pub async fn claim_pay(
        client: &impl GenericClient,
        bout_id: Uuid,
        user_id: Uuid,
        paid_bits: i64,
        now: DateTime<Utc>,
    ) -> Result<Option<BoutBet>> {
        let row = client
            .query_opt(
                "UPDATE deadchannel_bout_bets
                 SET paid_at = $3, paid_bits = $4
                 WHERE bout_id = $1 AND user_id = $2 AND paid_at IS NULL
                 RETURNING *",
                &[&bout_id, &user_id, &now, &paid_bits],
            )
            .await
            .context("claiming the bet's pay")?;
        row.as_ref().map(Self::from_row).transpose()
    }

    /// The bets on a bout nobody has paid yet, first placed first.
    pub async fn unpaid(client: &impl GenericClient, bout_id: Uuid) -> Result<Vec<BoutBet>> {
        let rows = client
            .query(
                "SELECT * FROM deadchannel_bout_bets
                 WHERE bout_id = $1 AND paid_at IS NULL
                 ORDER BY placed_at, user_id",
                &[&bout_id],
            )
            .await
            .context("listing the bout's unpaid bets")?;
        rows.iter().map(Self::from_row).collect()
    }
}

/// The deck a runner brings to the ring, as JSON the app reads. One row
/// per runner, copied into a bout when one is accepted.
pub struct RingDeck;

impl RingDeck {
    pub async fn get(client: &impl GenericClient, user_id: Uuid) -> Result<Option<Value>> {
        let row = client
            .query_opt(
                "SELECT cards FROM deadchannel_ring_decks WHERE user_id = $1",
                &[&user_id],
            )
            .await
            .context("reading the ring deck")?;
        Ok(row.map(|row| row.get("cards")))
    }

    pub async fn set(
        client: &impl GenericClient,
        user_id: Uuid,
        cards: &Value,
        now: DateTime<Utc>,
    ) -> Result<()> {
        client
            .execute(
                "INSERT INTO deadchannel_ring_decks (user_id, cards, updated_at)
                 VALUES ($1, $2, $3)
                 ON CONFLICT (user_id) DO UPDATE
                 SET cards = EXCLUDED.cards, updated_at = EXCLUDED.updated_at",
                &[&user_id, cards, &now],
            )
            .await
            .context("storing the ring deck")?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "deadchannel_bout_test.rs"]
mod deadchannel_bout_test;
