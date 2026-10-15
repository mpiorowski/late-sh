//! The lounge spawns (`app/deadchannel/lounge`, GAME.md "The lounge pass"):
//! the glyph that lands in #lounge for runners to put down together. This
//! module owns every read and write of `deadchannel_lounge_days` (the
//! day's budget and its dice) and `deadchannel_spawns` (the spawns), and
//! the channel a spawn's every change travels on.
//!
//! Unlike the name hit, a spawn is truth: who hit it decides who is paid,
//! so it is a row, and every change to it is one conditional statement.
//! The notify carries the whole row (`Spawn` as JSON), so a listener
//! paints without a lookup, and a replica that boots mid-spawn re-reads
//! the live row on its resync.

use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use tokio_postgres::{Client, GenericClient, Row};
use uuid::Uuid;

/// Cross-process channel for a spawn's every change: it landed, it was
/// hit, it went down, it slipped away. The payload is the row as JSON.
pub const DEADCHANNEL_SPAWN_CHANNEL: &str = "deadchannel_spawn";

/// What lands in the lounge.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpawnKind {
    /// Dies to one hit.
    Flicker,
    /// Needs the room: three hits from three runners.
    Howler,
}

impl SpawnKind {
    pub fn as_str(self) -> &'static str {
        match self {
            SpawnKind::Flicker => "flicker",
            SpawnKind::Howler => "howler",
        }
    }

    fn parse(value: &str) -> Result<SpawnKind> {
        match value {
            "flicker" => Ok(SpawnKind::Flicker),
            "howler" => Ok(SpawnKind::Howler),
            other => anyhow::bail!("unknown spawn kind {other:?}"),
        }
    }
}

/// How a spawn ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpawnOutcome {
    /// Put down: the hits made the count.
    Slain,
    /// Past its time with the count short.
    Slipped,
}

impl SpawnOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            SpawnOutcome::Slain => "slain",
            SpawnOutcome::Slipped => "slipped",
        }
    }

    fn parse(value: &str) -> Result<SpawnOutcome> {
        match value {
            "slain" => Ok(SpawnOutcome::Slain),
            "slipped" => Ok(SpawnOutcome::Slipped),
            other => anyhow::bail!("unknown spawn outcome {other:?}"),
        }
    }
}

/// The three eight-hour windows of a UTC day, one flicker each.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Window {
    /// 00:00 to 08:00 UTC.
    Early,
    /// 08:00 to 16:00 UTC.
    Mid,
    /// 16:00 to 24:00 UTC.
    Late,
}

/// Minutes in one window.
pub const WINDOW_MINUTES: i32 = 480;

impl Window {
    pub const ALL: [Window; 3] = [Window::Early, Window::Mid, Window::Late];

    /// The window a minute of the day (0 to 1439) falls in.
    pub fn of_minute(minute: i32) -> Window {
        match minute / WINDOW_MINUTES {
            0 => Window::Early,
            1 => Window::Mid,
            _ => Window::Late,
        }
    }

    /// The first minute of the day in this window.
    pub fn first_minute(self) -> i32 {
        self.index() * WINDOW_MINUTES
    }

    pub fn index(self) -> i32 {
        match self {
            Window::Early => 0,
            Window::Mid => 1,
            Window::Late => 2,
        }
    }

    fn spent_column(self) -> &'static str {
        match self {
            Window::Early => "flicker_0_spent",
            Window::Mid => "flicker_1_spent",
            Window::Late => "flicker_2_spent",
        }
    }
}

/// The day's dice, rolled by whoever inserts the day: the minute of the
/// day each window's flicker lands at (inside its window) and the
/// howler's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LoungeDice {
    pub flicker_minutes: [i32; 3],
    pub howler_minute: i32,
}

/// One UTC day's budget as the row holds it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoungeDay {
    pub day: NaiveDate,
    /// By `Window::index`.
    pub flicker_minutes: [i32; 3],
    pub howler_minute: i32,
    pub flickers_spent: [bool; 3],
    pub howler_spent: bool,
    pub last_spawn_at: Option<DateTime<Utc>>,
}

impl LoungeDay {
    fn from_row(row: &Row) -> Self {
        Self {
            day: row.get("day"),
            flicker_minutes: [
                row.get("flicker_0_minute"),
                row.get("flicker_1_minute"),
                row.get("flicker_2_minute"),
            ],
            howler_minute: row.get("howler_minute"),
            flickers_spent: [
                row.get("flicker_0_spent"),
                row.get("flicker_1_spent"),
                row.get("flicker_2_spent"),
            ],
            howler_spent: row.get("howler_spent"),
            last_spawn_at: row.get("last_spawn_at"),
        }
    }

    pub fn flicker_minute(&self, window: Window) -> i32 {
        self.flicker_minutes[window.index() as usize]
    }

    pub fn flicker_spent(&self, window: Window) -> bool {
        self.flickers_spent[window.index() as usize]
    }

    /// The day's row, inserted with `dice` if it is not there yet. A
    /// conditional insert: the first replica to touch the day rolls, every
    /// other reads what it rolled.
    pub async fn ensure(
        client: &impl GenericClient,
        day: NaiveDate,
        dice: LoungeDice,
    ) -> Result<LoungeDay> {
        client
            .execute(
                "INSERT INTO deadchannel_lounge_days
                    (day, flicker_0_minute, flicker_1_minute, flicker_2_minute, howler_minute)
                 VALUES ($1, $2, $3, $4, $5)
                 ON CONFLICT (day) DO NOTHING",
                &[
                    &day,
                    &dice.flicker_minutes[0],
                    &dice.flicker_minutes[1],
                    &dice.flicker_minutes[2],
                    &dice.howler_minute,
                ],
            )
            .await
            .context("inserting the lounge day")?;
        let row = client
            .query_one(
                "SELECT * FROM deadchannel_lounge_days WHERE day = $1",
                &[&day],
            )
            .await
            .context("reading the lounge day")?;
        Ok(Self::from_row(&row))
    }

    /// Spend `window`'s flicker, if it is unspent and the last spawn was
    /// at or before `gap_closed_at`. Returns whether this call spent it.
    pub async fn claim_flicker(
        client: &impl GenericClient,
        day: NaiveDate,
        window: Window,
        spawned_at: DateTime<Utc>,
        gap_closed_at: DateTime<Utc>,
    ) -> Result<bool> {
        let spent = window.spent_column();
        let statement = format!(
            "UPDATE deadchannel_lounge_days
             SET {spent} = true, last_spawn_at = $2
             WHERE day = $1 AND NOT {spent}
               AND (last_spawn_at IS NULL OR last_spawn_at <= $3)"
        );
        let updated = client
            .execute(&statement, &[&day, &spawned_at, &gap_closed_at])
            .await
            .context("claiming the lounge flicker")?;
        Ok(updated == 1)
    }

    /// Spend the day's howler, same terms. Returns whether this call spent
    /// it.
    pub async fn claim_howler(
        client: &impl GenericClient,
        day: NaiveDate,
        spawned_at: DateTime<Utc>,
        gap_closed_at: DateTime<Utc>,
    ) -> Result<bool> {
        let updated = client
            .execute(
                "UPDATE deadchannel_lounge_days
                 SET howler_spent = true, last_spawn_at = $2
                 WHERE day = $1 AND NOT howler_spent
                   AND (last_spawn_at IS NULL OR last_spawn_at <= $3)",
                &[&day, &spawned_at, &gap_closed_at],
            )
            .await
            .context("claiming the lounge howler")?;
        Ok(updated == 1)
    }
}

/// One hit on a spawn, as the row keeps it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpawnHit {
    pub user_id: Uuid,
    pub username: String,
    pub at_ms: i64,
}

/// One spawn, as it travels between replicas and as sessions hold it.
/// Times are unix ms so a session compares them to its own clock.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Spawn {
    pub id: Uuid,
    pub kind: SpawnKind,
    pub hits_needed: i32,
    pub hits: Vec<SpawnHit>,
    pub spawned_at_ms: i64,
    pub expires_at_ms: i64,
    pub ended_at_ms: Option<i64>,
    pub outcome: Option<SpawnOutcome>,
}

/// What one hit came back with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HitResult {
    /// The hit is on the row; the spawn after it (ended as slain if this
    /// was the last one needed).
    Landed(Spawn),
    /// This runner already hit it.
    AlreadyHit,
    /// Nothing live under that id: it ended, or expired.
    Gone,
}

impl Spawn {
    fn from_row(row: &Row) -> Result<Self> {
        let kind: String = row.get("kind");
        let outcome: Option<String> = row.get("outcome");
        let hits: serde_json::Value = row.get("hits");
        let spawned_at: DateTime<Utc> = row.get("spawned_at");
        let expires_at: DateTime<Utc> = row.get("expires_at");
        let ended_at: Option<DateTime<Utc>> = row.get("ended_at");
        Ok(Self {
            id: row.get("id"),
            kind: SpawnKind::parse(&kind)?,
            hits_needed: row.get("hits_needed"),
            hits: serde_json::from_value(hits).context("reading the spawn's hits")?,
            spawned_at_ms: spawned_at.timestamp_millis(),
            expires_at_ms: expires_at.timestamp_millis(),
            ended_at_ms: ended_at.map(|at| at.timestamp_millis()),
            outcome: outcome.as_deref().map(SpawnOutcome::parse).transpose()?,
        })
    }

    pub fn to_payload(&self) -> String {
        serde_json::to_string(self).expect("a spawn always serializes")
    }

    pub fn is_live(&self) -> bool {
        self.ended_at_ms.is_none()
    }

    pub fn hit_by(&self, user_id: Uuid) -> bool {
        self.hits.iter().any(|hit| hit.user_id == user_id)
    }

    /// Hits still needed to put it down.
    pub fn hits_left(&self) -> i32 {
        (self.hits_needed - self.hits.len() as i32).max(0)
    }

    /// Put a spawn on the row. The one-live index refuses a second while
    /// one is live; the caller's transaction rolls its claim back then.
    pub async fn insert(
        client: &impl GenericClient,
        id: Uuid,
        kind: SpawnKind,
        hits_needed: i32,
        spawned_at: DateTime<Utc>,
        expires_at: DateTime<Utc>,
    ) -> Result<Spawn> {
        let row = client
            .query_one(
                "INSERT INTO deadchannel_spawns (id, kind, hits_needed, spawned_at, expires_at)
                 VALUES ($1, $2, $3, $4, $5)
                 RETURNING *",
                &[&id, &kind.as_str(), &hits_needed, &spawned_at, &expires_at],
            )
            .await
            .context("inserting the lounge spawn")?;
        Self::from_row(&row)
    }

    /// The spawn that has not ended, if any. Past its time it is still
    /// live here until a sweeper ends it; the caller judges the clock.
    pub async fn live(client: &impl GenericClient) -> Result<Option<Spawn>> {
        let row = client
            .query_opt(
                "SELECT * FROM deadchannel_spawns WHERE ended_at IS NULL",
                &[],
            )
            .await
            .context("reading the live lounge spawn")?;
        row.as_ref().map(Self::from_row).transpose()
    }

    pub async fn get(client: &impl GenericClient, id: Uuid) -> Result<Option<Spawn>> {
        let row = client
            .query_opt("SELECT * FROM deadchannel_spawns WHERE id = $1", &[&id])
            .await
            .context("reading the lounge spawn")?;
        row.as_ref().map(Self::from_row).transpose()
    }

    /// One hit: appended to `hits` if the spawn is live, not past `now`,
    /// and this runner has not hit it; the same statement ends it as slain
    /// when the hit makes the count. One hit per runner per spawn is the
    /// rule that makes a howler need the room.
    pub async fn hit(
        client: &impl GenericClient,
        id: Uuid,
        hit: &SpawnHit,
        now: DateTime<Utc>,
    ) -> Result<HitResult> {
        let entry = serde_json::to_value(hit).expect("a hit always serializes");
        let row = client
            .query_opt(
                "UPDATE deadchannel_spawns
                 SET hits = hits || $2::jsonb,
                     ended_at = CASE WHEN jsonb_array_length(hits) + 1 >= hits_needed
                                     THEN $4::timestamptz ELSE NULL END,
                     outcome = CASE WHEN jsonb_array_length(hits) + 1 >= hits_needed
                                    THEN 'slain' ELSE NULL END
                 WHERE id = $1 AND ended_at IS NULL AND expires_at > $4
                   AND NOT EXISTS (
                       SELECT 1 FROM jsonb_array_elements(hits) AS h
                       WHERE h->>'user_id' = $3
                   )
                 RETURNING *",
                &[&id, &entry, &hit.user_id.to_string(), &now],
            )
            .await
            .context("hitting the lounge spawn")?;
        if let Some(row) = row {
            return Ok(HitResult::Landed(Self::from_row(&row)?));
        }
        match Self::get(client, id).await? {
            Some(spawn) if spawn.hit_by(hit.user_id) => Ok(HitResult::AlreadyHit),
            _ => Ok(HitResult::Gone),
        }
    }

    /// End every live spawn at or past its time as slipped. Returns the
    /// ones this call ended: whoever ends one says so on the wire.
    pub async fn end_expired(
        client: &impl GenericClient,
        now: DateTime<Utc>,
    ) -> Result<Vec<Spawn>> {
        let rows = client
            .query(
                "UPDATE deadchannel_spawns
                 SET ended_at = $1, outcome = 'slipped'
                 WHERE ended_at IS NULL AND expires_at <= $1
                 RETURNING *",
                &[&now],
            )
            .await
            .context("ending the expired lounge spawns")?;
        rows.iter().map(Self::from_row).collect()
    }

    /// Take the pay claim on a slain spawn. `Some` for the one caller that
    /// gets to pay its hitters; `None` for everyone after.
    pub async fn claim_pay(
        client: &impl GenericClient,
        id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<Option<Spawn>> {
        let row = client
            .query_opt(
                "UPDATE deadchannel_spawns
                 SET paid_at = $2
                 WHERE id = $1 AND outcome = 'slain' AND paid_at IS NULL
                 RETURNING *",
                &[&id, &now],
            )
            .await
            .context("claiming the lounge spawn's pay")?;
        row.as_ref().map(Self::from_row).transpose()
    }

    /// Slain spawns nobody has paid, ended at or before `ended_by`: what
    /// a replica that died mid-kill left behind, for any sweeper to settle.
    pub async fn unpaid_slain(
        client: &impl GenericClient,
        ended_by: DateTime<Utc>,
    ) -> Result<Vec<Spawn>> {
        let rows = client
            .query(
                "SELECT * FROM deadchannel_spawns
                 WHERE outcome = 'slain' AND paid_at IS NULL AND ended_at <= $1
                 ORDER BY ended_at",
                &[&ended_by],
            )
            .await
            .context("listing the unpaid lounge spawns")?;
        rows.iter().map(Self::from_row).collect()
    }
}

/// Fire a spawn's state at every replica, this one included: the
/// publisher's pooled connection is not the listener's, so the local
/// sessions hear it over the same wire everyone else does.
pub async fn notify_spawn(client: &Client, spawn: &Spawn) -> Result<()> {
    client
        .execute(
            "SELECT pg_notify($1, $2)",
            &[&DEADCHANNEL_SPAWN_CHANNEL, &spawn.to_payload()],
        )
        .await
        .context("notifying deadchannel spawn")?;
    Ok(())
}

/// The spawn carried by a [`DEADCHANNEL_SPAWN_CHANNEL`] payload. `None`
/// for anything this module did not write.
pub fn parse_spawn_payload(payload: &str) -> Option<Spawn> {
    serde_json::from_str(payload).ok()
}

#[cfg(test)]
#[path = "deadchannel_spawn_test.rs"]
mod deadchannel_spawn_test;
