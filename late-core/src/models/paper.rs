//! The Late Edition's printed pages (`paper_room_editions`,
//! `paper_sections`, migration 173): every read and write of both tables.
//!
//! A row is a claim. The sweeper in late-ssh `app/paper/svc.rs` inserts a
//! `printing` row before it spends a model call, so two replicas sweeping
//! the same minute cannot both pay for one room; the winner fills the text
//! in and flips the row to `ready`, a loser finds no row to insert. A print
//! that fails flips its row to `failed` and keeps the attempt count: the
//! next sweep claims it again until the caller's attempt cap, after which
//! the row is settled for the day. A `printing` row nobody finished (a
//! replica died mid-call) is reclaimed once it is older than the caller's
//! stale bound.

use anyhow::{Result, bail};
use chrono::{DateTime, NaiveDate, Utc};
use tokio_postgres::{Client, Row};
use uuid::Uuid;

/// Where a page is in its life. Stored as text, parsed on read, and never
/// defaulted: an unknown status in the table is a bug worth a crash.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaperStatus {
    /// Claimed; the model call is running (or the claimant died).
    Printing,
    /// Printed; `text` is set.
    Ready,
    /// Looked at and skipped: under the message threshold (no call
    /// spent), or the model had nothing usable to say.
    Quiet,
    /// The last print attempt failed. Claimed again by the next sweep
    /// while under the attempt cap; settled once at it.
    Failed,
}

impl PaperStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Printing => "printing",
            Self::Ready => "ready",
            Self::Quiet => "quiet",
            Self::Failed => "failed",
        }
    }

    fn parse(value: &str) -> Result<Self> {
        match value {
            "printing" => Ok(Self::Printing),
            "ready" => Ok(Self::Ready),
            "quiet" => Ok(Self::Quiet),
            "failed" => Ok(Self::Failed),
            other => bail!("unknown paper status {other:?}"),
        }
    }
}

/// The edition-level sections. A new section means a new variant, a wider
/// CHECK constraint in a migration, and a printer for it in the sweeper.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaperSectionKind {
    /// What the clubhouse shared into News that day.
    Reading,
    /// The grounded look at the outside world.
    Outside,
}

impl PaperSectionKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Reading => "reading",
            Self::Outside => "outside",
        }
    }

    fn parse(value: &str) -> Result<Self> {
        match value {
            "reading" => Ok(Self::Reading),
            "outside" => Ok(Self::Outside),
            other => bail!("unknown paper section {other:?}"),
        }
    }
}

/// A public room the sweeper has not settled for this edition: no row yet,
/// a `printing` row past the stale bound, or a `failed` row under the
/// attempt cap. Counts cover the edition's window and exclude system-feed
/// lines. Carries what a page needs, so an in-memory preview can lay one
/// out without a row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaperCandidate {
    pub room_id: Uuid,
    /// The room's rail name (slug, or the language code for language rooms).
    pub label: String,
    pub kind: String,
    pub permanent: bool,
    pub member_count: i64,
    pub message_count: i64,
    pub author_count: i64,
}

/// One room's page as read back for a reader, joined to the room so the
/// paper can name it and say how many people are inside.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaperRoomPage {
    pub room_id: Uuid,
    pub label: String,
    pub member_count: i64,
    /// A shop `room_bump` could put a room at the top of "Elsewhere";
    /// permanent rooms and non-topic rooms never carry one.
    pub kind: String,
    pub permanent: bool,
    pub status: PaperStatus,
    pub message_count: i64,
    pub author_count: i64,
    pub text: Option<String>,
}

impl PaperRoomPage {
    fn from_row(row: Row) -> Result<Self> {
        let status: String = row.get("status");
        Ok(Self {
            room_id: row.get("room_id"),
            label: row.get("label"),
            member_count: row.get("member_count"),
            kind: row.get("kind"),
            permanent: row.get("permanent"),
            status: PaperStatus::parse(&status)?,
            message_count: i64::from(row.get::<_, i32>("message_count")),
            author_count: i64::from(row.get::<_, i32>("author_count")),
            text: row.get("text"),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaperSection {
    pub section: PaperSectionKind,
    pub status: PaperStatus,
    pub text: Option<String>,
}

impl PaperSection {
    fn from_row(row: Row) -> Result<Self> {
        let section: String = row.get("section");
        let status: String = row.get("status");
        Ok(Self {
            section: PaperSectionKind::parse(&section)?,
            status: PaperStatus::parse(&status)?,
            text: row.get("text"),
        })
    }
}

/// Everything printed for one edition, in one read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaperEdition {
    pub edition: NaiveDate,
    pub rooms: Vec<PaperRoomPage>,
    pub sections: Vec<PaperSection>,
}

impl PaperEdition {
    /// Both tables for `edition`. Rooms come back only while still public:
    /// a room made private after its page was printed keeps the row and
    /// loses the reader, the same line `/summary` draws.
    pub async fn load(client: &Client, edition: NaiveDate) -> Result<Self> {
        let room_rows = client
            .query(
                "SELECT e.room_id,
                        COALESCE(r.slug, r.language_code) AS label,
                        r.kind,
                        r.permanent,
                        e.status,
                        e.message_count,
                        e.author_count,
                        e.text,
                        (SELECT COUNT(*)::bigint FROM chat_room_members m
                          WHERE m.room_id = r.id) AS member_count
                 FROM paper_room_editions e
                 JOIN chat_rooms r ON r.id = e.room_id
                 WHERE e.edition = $1
                   AND r.visibility = 'public'
                   AND COALESCE(r.slug, r.language_code) IS NOT NULL
                 ORDER BY e.message_count DESC, label ASC",
                &[&edition],
            )
            .await?;
        let mut rooms = Vec::with_capacity(room_rows.len());
        for row in room_rows {
            rooms.push(PaperRoomPage::from_row(row)?);
        }
        let section_rows = client
            .query(
                "SELECT section, status, text
                 FROM paper_sections
                 WHERE edition = $1
                 ORDER BY section ASC",
                &[&edition],
            )
            .await?;
        let mut sections = Vec::with_capacity(section_rows.len());
        for row in section_rows {
            sections.push(PaperSection::from_row(row)?);
        }
        Ok(Self {
            edition,
            rooms,
            sections,
        })
    }

    /// True when a reader would get at least one printed page.
    pub fn has_print(&self) -> bool {
        self.rooms
            .iter()
            .any(|page| page.status == PaperStatus::Ready)
            || self
                .sections
                .iter()
                .any(|section| section.status == PaperStatus::Ready)
    }
}

pub struct PaperRoomEdition;

impl PaperRoomEdition {
    /// Public rooms with at least one human message inside
    /// `[floor, ceiling)` that this edition has not settled: no row, a
    /// `printing` claim older than `stale_before`, or a `failed` row with
    /// fewer than `max_attempts` claims. Public lounge, topic, and language
    /// rooms only; DMs, game rooms, and the haunted channel never reach the
    /// paper.
    pub async fn list_candidates(
        client: &Client,
        edition: NaiveDate,
        floor: DateTime<Utc>,
        ceiling: DateTime<Utc>,
        stale_before: DateTime<Utc>,
        max_attempts: i32,
    ) -> Result<Vec<PaperCandidate>> {
        let rows = client
            .query(
                "SELECT r.id AS room_id,
                        COALESCE(r.slug, r.language_code) AS label,
                        r.kind,
                        r.permanent,
                        (SELECT COUNT(*)::bigint FROM chat_room_members m
                          WHERE m.room_id = r.id) AS member_count,
                        COUNT(*)::bigint AS message_count,
                        COUNT(DISTINCT msg.user_id)::bigint AS author_count
                 FROM chat_rooms r
                 JOIN chat_messages msg ON msg.room_id = r.id
                 JOIN users author ON author.id = msg.user_id
                 WHERE r.visibility = 'public'
                   AND r.kind IN ('lounge', 'topic', 'language')
                   AND COALESCE(r.slug, r.language_code) IS NOT NULL
                   AND msg.created >= $2
                   AND msg.created < $3
                   AND COALESCE((author.settings->>'system')::boolean, false) = false
                   AND NOT EXISTS (
                        SELECT 1 FROM paper_room_editions e
                        WHERE e.room_id = r.id
                          AND e.edition = $1
                          AND (e.status IN ('ready', 'quiet')
                               OR (e.status = 'printing' AND e.claimed_at >= $4)
                               OR (e.status = 'failed' AND e.attempts >= $5))
                   )
                 GROUP BY r.id, label
                 ORDER BY message_count DESC, label ASC",
                &[&edition, &floor, &ceiling, &stale_before, &max_attempts],
            )
            .await?;
        Ok(rows
            .into_iter()
            .map(|row| PaperCandidate {
                room_id: row.get("room_id"),
                label: row.get("label"),
                kind: row.get("kind"),
                permanent: row.get("permanent"),
                member_count: row.get("member_count"),
                message_count: row.get("message_count"),
                author_count: row.get("author_count"),
            })
            .collect())
    }

    /// Take the room's page for this edition. Wins when no row exists, the
    /// existing claim is `printing` and older than `stale_before`, or the
    /// row is `failed` with fewer than `max_attempts` claims; the win is
    /// the only licence to spend the model call. Every win counts an
    /// attempt.
    pub async fn claim_printing(
        client: &Client,
        room_id: Uuid,
        edition: NaiveDate,
        message_count: i64,
        author_count: i64,
        stale_before: DateTime<Utc>,
        max_attempts: i32,
    ) -> Result<bool> {
        let message_count = i32::try_from(message_count)?;
        let author_count = i32::try_from(author_count)?;
        let claimed = client
            .execute(
                "INSERT INTO paper_room_editions
                    (room_id, edition, status, message_count, author_count, attempts, claimed_at)
                 VALUES ($1, $2, 'printing', $3, $4, 1, current_timestamp)
                 ON CONFLICT (room_id, edition) DO UPDATE
                    SET status = 'printing',
                        claimed_at = current_timestamp,
                        attempts = paper_room_editions.attempts + 1,
                        message_count = EXCLUDED.message_count,
                        author_count = EXCLUDED.author_count
                    WHERE (paper_room_editions.status = 'printing'
                           AND paper_room_editions.claimed_at < $5)
                       OR (paper_room_editions.status = 'failed'
                           AND paper_room_editions.attempts < $6)",
                &[
                    &room_id,
                    &edition,
                    &message_count,
                    &author_count,
                    &stale_before,
                    &max_attempts,
                ],
            )
            .await?;
        Ok(claimed == 1)
    }

    /// Record that the room fell under the threshold: no call spent, and
    /// the next sweep skips it. Takes over a `printing` claim older than
    /// `stale_before` or a `failed` row (the room is under the threshold
    /// now, whatever it was); a `ready` or `quiet` row stands.
    pub async fn mark_quiet(
        client: &Client,
        room_id: Uuid,
        edition: NaiveDate,
        message_count: i64,
        author_count: i64,
        stale_before: DateTime<Utc>,
    ) -> Result<bool> {
        let message_count = i32::try_from(message_count)?;
        let author_count = i32::try_from(author_count)?;
        let settled = client
            .execute(
                "INSERT INTO paper_room_editions
                    (room_id, edition, status, message_count, author_count, attempts,
                     claimed_at, generated_at)
                 VALUES ($1, $2, 'quiet', $3, $4, 0, current_timestamp, current_timestamp)
                 ON CONFLICT (room_id, edition) DO UPDATE
                    SET status = 'quiet',
                        generated_at = current_timestamp,
                        message_count = EXCLUDED.message_count,
                        author_count = EXCLUDED.author_count
                    WHERE (paper_room_editions.status = 'printing'
                           AND paper_room_editions.claimed_at < $5)
                       OR paper_room_editions.status = 'failed'",
                &[
                    &room_id,
                    &edition,
                    &message_count,
                    &author_count,
                    &stale_before,
                ],
            )
            .await?;
        Ok(settled == 1)
    }

    /// The winner's page: `ready` with the text, or `quiet` when the model
    /// had nothing usable to say, so no call is spent on the room again.
    /// Bails when the claim is no longer held (reclaimed as stale by
    /// another replica): the page is then the other replica's to write.
    pub async fn finish(
        client: &Client,
        room_id: Uuid,
        edition: NaiveDate,
        text: Option<&str>,
    ) -> Result<()> {
        let status = match text {
            Some(_) => PaperStatus::Ready,
            None => PaperStatus::Quiet,
        };
        let updated = client
            .execute(
                "UPDATE paper_room_editions
                 SET status = $3, text = $4, generated_at = current_timestamp
                 WHERE room_id = $1 AND edition = $2 AND status = 'printing'",
                &[&room_id, &edition, &status.as_str(), &text],
            )
            .await?;
        if updated == 0 {
            bail!("paper claim for room {room_id} edition {edition} no longer held");
        }
        Ok(())
    }

    /// Drop every room row of `edition` (the admin `/paper reset` hook),
    /// so the next sweep or `/paper print` prints it again. Returns how
    /// many rows went.
    pub async fn delete_edition(client: &Client, edition: NaiveDate) -> Result<u64> {
        let deleted = client
            .execute(
                "DELETE FROM paper_room_editions WHERE edition = $1",
                &[&edition],
            )
            .await?;
        Ok(deleted)
    }

    /// Record a failed print under the held claim. The attempt count
    /// stays, so the next sweep retries only while under the cap.
    pub async fn mark_failed(client: &Client, room_id: Uuid, edition: NaiveDate) -> Result<()> {
        client
            .execute(
                "UPDATE paper_room_editions
                 SET status = 'failed'
                 WHERE room_id = $1 AND edition = $2 AND status = 'printing'",
                &[&room_id, &edition],
            )
            .await?;
        Ok(())
    }
}

pub struct PaperSectionRow;

impl PaperSectionRow {
    /// Whether this edition still needs `section` printed: no row, a
    /// `printing` claim older than `stale_before`, or a `failed` row with
    /// fewer than `max_attempts` claims.
    pub async fn is_unsettled(
        client: &Client,
        edition: NaiveDate,
        section: PaperSectionKind,
        stale_before: DateTime<Utc>,
        max_attempts: i32,
    ) -> Result<bool> {
        let settled = client
            .query_opt(
                "SELECT 1 FROM paper_sections
                 WHERE edition = $1 AND section = $2
                   AND (status IN ('ready', 'quiet')
                        OR (status = 'printing' AND claimed_at >= $3)
                        OR (status = 'failed' AND attempts >= $4))",
                &[&edition, &section.as_str(), &stale_before, &max_attempts],
            )
            .await?;
        Ok(settled.is_none())
    }

    /// Same contract as [`PaperRoomEdition::claim_printing`].
    pub async fn claim_printing(
        client: &Client,
        edition: NaiveDate,
        section: PaperSectionKind,
        stale_before: DateTime<Utc>,
        max_attempts: i32,
    ) -> Result<bool> {
        let claimed = client
            .execute(
                "INSERT INTO paper_sections (edition, section, status, attempts, claimed_at)
                 VALUES ($1, $2, 'printing', 1, current_timestamp)
                 ON CONFLICT (edition, section) DO UPDATE
                    SET status = 'printing',
                        claimed_at = current_timestamp,
                        attempts = paper_sections.attempts + 1
                    WHERE (paper_sections.status = 'printing'
                           AND paper_sections.claimed_at < $3)
                       OR (paper_sections.status = 'failed'
                           AND paper_sections.attempts < $4)",
                &[&edition, &section.as_str(), &stale_before, &max_attempts],
            )
            .await?;
        Ok(claimed == 1)
    }

    /// Same contract as [`PaperRoomEdition::finish`]: `quiet` when the
    /// print found nothing to say (nobody shared, the outside world
    /// reported nothing dated), so no call is spent on it again.
    pub async fn finish(
        client: &Client,
        edition: NaiveDate,
        section: PaperSectionKind,
        text: Option<&str>,
    ) -> Result<()> {
        let status = match text {
            Some(_) => PaperStatus::Ready,
            None => PaperStatus::Quiet,
        };
        let updated = client
            .execute(
                "UPDATE paper_sections
                 SET status = $3, text = $4, generated_at = current_timestamp
                 WHERE edition = $1 AND section = $2 AND status = 'printing'",
                &[&edition, &section.as_str(), &status.as_str(), &text],
            )
            .await?;
        if updated == 0 {
            bail!(
                "paper claim for section {} edition {edition} no longer held",
                section.as_str()
            );
        }
        Ok(())
    }

    /// The newest `limit` printed pages of `section` from editions before
    /// `edition`, newest first: the press's memory, so a column does not
    /// repeat what an earlier edition already said.
    pub async fn list_recent_ready(
        client: &Client,
        section: PaperSectionKind,
        before: NaiveDate,
        limit: i64,
    ) -> Result<Vec<(NaiveDate, String)>> {
        let rows = client
            .query(
                "SELECT edition, text
                 FROM paper_sections
                 WHERE section = $1 AND status = 'ready' AND edition < $2
                 ORDER BY edition DESC
                 LIMIT $3",
                &[&section.as_str(), &before, &limit],
            )
            .await?;
        Ok(rows
            .into_iter()
            .map(|row| (row.get("edition"), row.get("text")))
            .collect())
    }

    /// Same contract as [`PaperRoomEdition::delete_edition`].
    pub async fn delete_edition(client: &Client, edition: NaiveDate) -> Result<u64> {
        let deleted = client
            .execute("DELETE FROM paper_sections WHERE edition = $1", &[&edition])
            .await?;
        Ok(deleted)
    }

    /// Same contract as [`PaperRoomEdition::mark_failed`].
    pub async fn mark_failed(
        client: &Client,
        edition: NaiveDate,
        section: PaperSectionKind,
    ) -> Result<()> {
        client
            .execute(
                "UPDATE paper_sections
                 SET status = 'failed'
                 WHERE edition = $1 AND section = $2 AND status = 'printing'",
                &[&edition, &section.as_str()],
            )
            .await?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "paper_test.rs"]
mod paper_test;
