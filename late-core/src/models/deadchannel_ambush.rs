//! The crossing (`app/deadchannel/fight`, GAME.md "The crossing pass"):
//! one runner fighting another while they sleep, and the answer the
//! sleeper is owed. This module owns every read and write of
//! `deadchannel_ambushes`.
//!
//! A row is one fight between two runners: who started it, who it was
//! on, what was at stake, and how it ended. The three unique indexes of
//! migration 240 are the day's caps (a sleeper crossed once a day, a pair
//! crossing once a day, a crossing answered once), so an insert that
//! loses one is the cap refusing, not a bug.

use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDate, Utc};
use tokio_postgres::{GenericClient, Row};
use uuid::Uuid;

/// How a fight between two runners ended, from the attacker's side.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AmbushOutcome {
    /// The attacker put the defender down and took the stake.
    Won,
    /// The attacker went down; what they carried went to the defender.
    Lost,
    /// The attacker ran. Nothing moved.
    Ran,
    /// The day rolled over the fight before it ended. Nothing moved.
    Dropped,
}

impl AmbushOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            AmbushOutcome::Won => "won",
            AmbushOutcome::Lost => "lost",
            AmbushOutcome::Ran => "ran",
            AmbushOutcome::Dropped => "dropped",
        }
    }

    fn parse(value: &str) -> Result<AmbushOutcome> {
        match value {
            "won" => Ok(AmbushOutcome::Won),
            "lost" => Ok(AmbushOutcome::Lost),
            "ran" => Ok(AmbushOutcome::Ran),
            "dropped" => Ok(AmbushOutcome::Dropped),
            other => anyhow::bail!("unknown ambush outcome {other:?}"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ambush {
    pub id: Uuid,
    pub day: NaiveDate,
    pub attacker_user_id: Uuid,
    pub defender_user_id: Uuid,
    pub attacker_username: String,
    pub defender_username: String,
    /// The step of the road the crossing was on; `None` for an answer.
    pub step: Option<i32>,
    /// The most a win takes: the crossing's half of the defender's bits
    /// on hand when it started.
    pub stake: i64,
    /// The crossing this answers; `None` for a crossing.
    pub answer_to: Option<Uuid>,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub outcome: Option<AmbushOutcome>,
    /// What changed hands, from the attacker's side: what they took, or
    /// less what they lost.
    pub bits_moved: i64,
}

/// What a new row says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewAmbush {
    pub id: Uuid,
    pub day: NaiveDate,
    pub attacker_user_id: Uuid,
    pub defender_user_id: Uuid,
    pub attacker_username: String,
    pub defender_username: String,
    pub step: Option<i32>,
    pub stake: i64,
    pub answer_to: Option<Uuid>,
    pub started_at: DateTime<Utc>,
}

impl Ambush {
    fn from_row(row: &Row) -> Result<Self> {
        let outcome: Option<String> = row.get("outcome");
        Ok(Self {
            id: row.get("id"),
            day: row.get("day"),
            attacker_user_id: row.get("attacker_user_id"),
            defender_user_id: row.get("defender_user_id"),
            attacker_username: row.get("attacker_username"),
            defender_username: row.get("defender_username"),
            step: row.get("step"),
            stake: row.get("stake"),
            answer_to: row.get("answer_to"),
            started_at: row.get("started_at"),
            ended_at: row.get("ended_at"),
            outcome: outcome.as_deref().map(AmbushOutcome::parse).transpose()?,
            bits_moved: row.get("bits_moved"),
        })
    }

    pub fn is_answer(&self) -> bool {
        self.answer_to.is_some()
    }

    /// Put a fight on the table. `Ok(None)` when one of the day's caps
    /// refused it: the sleeper was crossed today already, the pair
    /// crossed today already, or the crossing was answered already.
    pub async fn insert(client: &impl GenericClient, new: &NewAmbush) -> Result<Option<Ambush>> {
        let row = client
            .query_opt(
                "INSERT INTO deadchannel_ambushes
                     (id, day, attacker_user_id, defender_user_id, attacker_username,
                      defender_username, step, stake, answer_to, started_at)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
                 ON CONFLICT DO NOTHING
                 RETURNING *",
                &[
                    &new.id,
                    &new.day,
                    &new.attacker_user_id,
                    &new.defender_user_id,
                    &new.attacker_username,
                    &new.defender_username,
                    &new.step,
                    &new.stake,
                    &new.answer_to,
                    &new.started_at,
                ],
            )
            .await
            .context("inserting the ambush")?;
        row.as_ref().map(Self::from_row).transpose()
    }

    /// End a fight, once: conditional on it still being open. Returns
    /// whether this call ended it.
    pub async fn end(
        client: &impl GenericClient,
        id: Uuid,
        outcome: AmbushOutcome,
        bits_moved: i64,
        now: DateTime<Utc>,
    ) -> Result<bool> {
        let row = client
            .query_opt(
                "UPDATE deadchannel_ambushes
                 SET ended_at = $2, outcome = $3, bits_moved = $4
                 WHERE id = $1 AND ended_at IS NULL
                 RETURNING id",
                &[&id, &now, &outcome.as_str(), &bits_moved],
            )
            .await
            .context("ending the ambush")?;
        Ok(row.is_some())
    }

    pub async fn get(client: &impl GenericClient, id: Uuid) -> Result<Option<Ambush>> {
        let row = client
            .query_opt("SELECT * FROM deadchannel_ambushes WHERE id = $1", &[&id])
            .await
            .context("reading the ambush")?;
        row.as_ref().map(Self::from_row).transpose()
    }

    /// Every crossing of `day` (answers left out): what the day's caps
    /// have counted so far, for picking who can still be crossed.
    pub async fn crossings_of_day(
        client: &impl GenericClient,
        day: NaiveDate,
    ) -> Result<Vec<Ambush>> {
        let rows = client
            .query(
                "SELECT * FROM deadchannel_ambushes
                 WHERE day = $1 AND answer_to IS NULL",
                &[&day],
            )
            .await
            .context("reading the day's crossings")?;
        rows.iter().map(Self::from_row).collect()
    }

    /// The latest fight on `defender` that ended at or after `since`: the
    /// news their row reads them.
    pub async fn latest_on(
        client: &impl GenericClient,
        defender_user_id: Uuid,
        since: DateTime<Utc>,
    ) -> Result<Option<Ambush>> {
        let row = client
            .query_opt(
                "SELECT * FROM deadchannel_ambushes
                 WHERE defender_user_id = $1 AND ended_at >= $2
                 ORDER BY ended_at DESC
                 LIMIT 1",
                &[&defender_user_id, &since],
            )
            .await
            .context("reading the latest ambush on a runner")?;
        row.as_ref().map(Self::from_row).transpose()
    }

    /// The crossing `defender` is owed an answer for: won by the
    /// attacker, ended at or after `since`, and not answered yet. The
    /// latest, if more than one.
    pub async fn answerable(
        client: &impl GenericClient,
        defender_user_id: Uuid,
        since: DateTime<Utc>,
    ) -> Result<Option<Ambush>> {
        let row = client
            .query_opt(
                "SELECT * FROM deadchannel_ambushes AS crossing
                 WHERE crossing.defender_user_id = $1
                   AND crossing.answer_to IS NULL
                   AND crossing.outcome = 'won'
                   AND crossing.ended_at >= $2
                   AND NOT EXISTS (
                       SELECT 1 FROM deadchannel_ambushes AS answer
                       WHERE answer.answer_to = crossing.id
                   )
                 ORDER BY crossing.ended_at DESC
                 LIMIT 1",
                &[&defender_user_id, &since],
            )
            .await
            .context("reading the crossing owed an answer")?;
        row.as_ref().map(Self::from_row).transpose()
    }
}

#[cfg(test)]
#[path = "deadchannel_ambush_test.rs"]
mod deadchannel_ambush_test;
