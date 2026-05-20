use anyhow::Result;
use tokio_postgres::Client;
use uuid::Uuid;

crate::model! {
    table = "media_queue_votes";
    params = MediaQueueVoteParams;
    struct MediaQueueVote {
        @data
        pub item_id: Uuid,
        pub user_id: Uuid,
        pub value: i16,
    }
}

/// Outcome of a guarded vote upsert. The `*_status` variants disambiguate
/// the rejection reason so callers in `late-ssh` can produce specific banner
/// copy without inspecting raw error strings.
pub enum CastVoteOutcome {
    Applied(i32),
    NotFound,
    VotingClosed,
    NotVoteable,
}

impl MediaQueueVote {
    pub async fn upsert(client: &Client, user_id: Uuid, item_id: Uuid, value: i16) -> Result<i32> {
        client
            .execute(
                "INSERT INTO media_queue_votes (user_id, item_id, value)
                 VALUES ($1, $2, $3)
                 ON CONFLICT (user_id, item_id)
                 DO UPDATE SET value = EXCLUDED.value,
                               updated = current_timestamp",
                &[&user_id, &item_id, &value],
            )
            .await?;
        Self::aggregate_for_item(client, item_id).await
    }

    /// Atomically check the parent item's status and upsert the vote. Locks
    /// the queue item row `FOR UPDATE` so a `queued -> playing` flip cannot
    /// race past the voting-closed guard.
    pub async fn cast_guarded(
        client: &mut Client,
        user_id: Uuid,
        item_id: Uuid,
        value: i16,
    ) -> Result<CastVoteOutcome> {
        let tx = client.transaction().await?;
        let row = tx
            .query_opt(
                "SELECT status FROM media_queue_items WHERE id = $1 FOR UPDATE",
                &[&item_id],
            )
            .await?;
        let Some(row) = row else {
            return Ok(CastVoteOutcome::NotFound);
        };
        let status: String = row.get("status");
        if status == "playing" {
            return Ok(CastVoteOutcome::VotingClosed);
        }
        if status != "queued" {
            return Ok(CastVoteOutcome::NotVoteable);
        }
        tx.execute(
            "INSERT INTO media_queue_votes (user_id, item_id, value)
             VALUES ($1, $2, $3)
             ON CONFLICT (user_id, item_id)
             DO UPDATE SET value = EXCLUDED.value,
                           updated = current_timestamp",
            &[&user_id, &item_id, &value],
        )
        .await?;
        let score_row = tx
            .query_one(
                "SELECT COALESCE(SUM(value), 0)::int AS score
                 FROM media_queue_votes WHERE item_id = $1",
                &[&item_id],
            )
            .await?;
        let score: i32 = score_row.get("score");
        tx.commit().await?;
        Ok(CastVoteOutcome::Applied(score))
    }

    pub async fn delete_vote(client: &Client, user_id: Uuid, item_id: Uuid) -> Result<i32> {
        client
            .execute(
                "DELETE FROM media_queue_votes
                 WHERE user_id = $1 AND item_id = $2",
                &[&user_id, &item_id],
            )
            .await?;
        Self::aggregate_for_item(client, item_id).await
    }

    pub async fn aggregate_for_item(client: &Client, item_id: Uuid) -> Result<i32> {
        let row = client
            .query_one(
                "SELECT COALESCE(SUM(value), 0)::int AS score
                 FROM media_queue_votes
                 WHERE item_id = $1",
                &[&item_id],
            )
            .await?;
        Ok(row.get::<_, i32>("score"))
    }

    pub async fn user_vote(client: &Client, user_id: Uuid, item_id: Uuid) -> Result<Option<i16>> {
        let row = client
            .query_opt(
                "SELECT value FROM media_queue_votes
                 WHERE user_id = $1 AND item_id = $2",
                &[&user_id, &item_id],
            )
            .await?;
        Ok(row.map(|r| r.get::<_, i16>("value")))
    }
}
