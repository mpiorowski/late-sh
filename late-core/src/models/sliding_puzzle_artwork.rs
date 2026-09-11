use anyhow::{Result, bail};
use chrono::NaiveDate;
use deadpool_postgres::GenericClient;
use tokio_postgres::Row;
use uuid::Uuid;

use super::chat_message::{ChatMessage, ChatMessageParams};

pub const SUBMISSION_ROOM: &str = "puzzle-art";

#[derive(Clone, Debug)]
pub struct Artwork {
    pub id: Uuid,
    pub title: String,
    pub credit: String,
    pub image_url: Option<String>,
    pub embedded_key: Option<String>,
}

impl From<Row> for Artwork {
    fn from(row: Row) -> Self {
        Self {
            id: row.get("id"),
            title: row.get("title"),
            credit: row.get("credit"),
            image_url: row.get("image_url"),
            embedded_key: row.get("embedded_key"),
        }
    }
}

pub struct Submission {
    pub source_url: String,
    pub image_url: String,
    pub sha256: String,
    pub title: String,
}

impl Artwork {
    /// Called inside the same transaction that publishes the review message.
    pub async fn submit(
        client: &impl GenericClient,
        room_id: Uuid,
        user_id: Uuid,
        submission: &Submission,
    ) -> Result<ChatMessage> {
        // Concurrent identical submissions must not publish duplicate cards.
        client
            .query_one(
                "SELECT pg_advisory_xact_lock(hashtextextended($1, 0))",
                &[&submission.sha256],
            )
            .await?;
        if client
            .query_opt(
                "SELECT id FROM sliding_puzzle_artworks WHERE sha256 = $1 AND active",
                &[&submission.sha256],
            )
            .await?
            .is_some()
        {
            bail!("puzzle-art:This image has already been submitted.");
        }
        let author = client
            .query_opt(
                "SELECT u.username FROM users u JOIN chat_room_members m ON m.user_id = u.id
             JOIN chat_rooms r ON r.id = m.room_id
             WHERE u.id = $1 AND r.id = $2 AND r.slug = 'puzzle-art'
               AND r.kind = 'topic' AND r.visibility = 'public'",
                &[&user_id, &room_id],
            )
            .await?
            .ok_or_else(|| anyhow::anyhow!("puzzle-art:Join #puzzle-art before submitting."))?;
        let credit: String = author.get("username");
        let message = ChatMessage::create_with_reply_targets(
            client,
            ChatMessageParams {
                room_id,
                user_id,
                body: format!("{}\n{}", submission.title, submission.image_url),
            },
            None,
            None,
        )
        .await?;
        client
            .execute(
                "INSERT INTO sliding_puzzle_artworks
             (source_message_id, source_user_id, source_url, image_url, sha256, title, credit)
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
                &[
                    &message.id,
                    &user_id,
                    &submission.source_url,
                    &submission.image_url,
                    &submission.sha256,
                    &submission.title,
                    &credit,
                ],
            )
            .await?;
        Ok(message)
    }

    pub async fn is_submission(client: &impl GenericClient, message_id: Uuid) -> Result<bool> {
        Ok(client
            .query_opt(
                "SELECT id FROM sliding_puzzle_artworks WHERE source_message_id = $1",
                &[&message_id],
            )
            .await?
            .is_some())
    }

    pub async fn approval_for_message(
        client: &impl GenericClient,
        message_id: Uuid,
        reviewer_id: Uuid,
    ) -> Result<Option<(String, NaiveDate)>> {
        Ok(client
            .query_opt(
                "SELECT title, available_from FROM sliding_puzzle_artworks
             WHERE source_message_id = $1 AND approved_at IS NOT NULL AND active
               AND EXISTS (SELECT 1 FROM users WHERE id = $2 AND (is_admin OR is_moderator))",
                &[&message_id, &reviewer_id],
            )
            .await?
            .map(|row| (row.get("title"), row.get("available_from"))))
    }

    /// Claim one immutable daily assignment across replicas. Use the least
    /// recently featured eligible image; newly approved art joins from tomorrow.
    /// The caller owns the transaction and commits before exposing the result.
    pub async fn assign_daily(client: &impl GenericClient, date: NaiveDate) -> Result<Artwork> {
        client
            .query_one(
                "SELECT pg_advisory_xact_lock(hashtextextended('sliding_puzzle_daily_artwork', 0))",
                &[],
            )
            .await?;
        if let Some(row) = client
            .query_opt(
                "SELECT a.* FROM sliding_puzzle_daily_artworks d
             JOIN sliding_puzzle_artworks a ON a.id = d.artwork_id WHERE d.puzzle_date = $1",
                &[&date],
            )
            .await?
        {
            return Ok(row.into());
        }
        let artwork: Artwork = client.query_one(
            "SELECT a.* FROM sliding_puzzle_artworks a
             LEFT JOIN LATERAL (SELECT MAX(puzzle_date) AS last_used FROM sliding_puzzle_daily_artworks
                                WHERE artwork_id = a.id AND puzzle_date < $1) d ON true
             WHERE a.active AND a.approved_at IS NOT NULL AND a.available_from <= $1
               AND NOT EXISTS (SELECT 1 FROM sliding_puzzle_daily_artworks neighbor
                               WHERE neighbor.artwork_id = a.id
                                 AND neighbor.puzzle_date IN ($1::date - 1, $1::date + 1))
             ORDER BY d.last_used ASC NULLS FIRST, a.created, a.id LIMIT 1", &[&date],
        ).await?.into();
        client.execute("INSERT INTO sliding_puzzle_daily_artworks (puzzle_date, artwork_id) VALUES ($1, $2)", &[&date, &artwork.id]).await?;
        Ok(artwork)
    }
}
