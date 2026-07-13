use anyhow::Result;
use chrono::{DateTime, Utc};
use deadpool_postgres::GenericClient;
use serde_json::Value;
use tokio_postgres::Client;
use uuid::Uuid;

crate::model! {
    table = "daily_matches";
    params = DailyMatchParams;
    struct DailyMatch {
        @data
        pub game_kind: String,
        pub status: String,
        pub challenger_id: Uuid,
        pub opponent_id: Option<Uuid>,
        pub target_user_id: Option<Uuid>,
        pub turn_user_id: Option<Uuid>,
        pub turn_deadline_at: Option<DateTime<Utc>>,
        pub winner_user_id: Option<Uuid>,
        pub result: String,
        pub state: Value,
        pub challenger_result_seen_at: Option<DateTime<Utc>>,
        pub opponent_result_seen_at: Option<DateTime<Utc>>,
        pub chat_room_id: Option<Uuid>,
    }
}

impl DailyMatch {
    pub const STATUS_OPEN: &'static str = "open";
    pub const STATUS_ACTIVE: &'static str = "active";
    pub const STATUS_FINISHED: &'static str = "finished";
    pub const STATUS_CANCELLED: &'static str = "cancelled";

    pub const RESULT_CHECKMATE: &'static str = "checkmate";
    pub const RESULT_DRAW: &'static str = "draw";
    pub const RESULT_RESIGN: &'static str = "resign";
    pub const RESULT_TIMEOUT: &'static str = "timeout";
    pub const RESULT_FLEET_SUNK: &'static str = "fleet_sunk";
    pub const RESULT_FOUR_IN_A_ROW: &'static str = "four_in_a_row";

    pub const GAME_KIND_CHESS: &'static str = "chess";
    pub const GAME_KIND_BATTLESHIP: &'static str = "battleship";
    pub const GAME_KIND_CONNECTFOUR: &'static str = "connect4";

    /// Open challenges posted by the user plus active matches they play in.
    pub async fn count_active_entries(client: &Client, user_id: Uuid) -> Result<i64> {
        let row = client
            .query_one(
                "SELECT COUNT(*)::bigint AS count
                 FROM daily_matches
                 WHERE (status = 'open' AND challenger_id = $1)
                    OR (status = 'active' AND (challenger_id = $1 OR opponent_id = $1))",
                &[&user_id],
            )
            .await?;
        let count: i64 = row.get("count");
        Ok(count)
    }

    pub async fn create_challenge(
        client: &Client,
        game_kind: &str,
        challenger_id: Uuid,
        target_user_id: Option<Uuid>,
    ) -> Result<Self> {
        let row = client
            .query_one(
                "INSERT INTO daily_matches (game_kind, status, challenger_id, target_user_id)
                 VALUES ($1, $2, $3, $4)
                 RETURNING *",
                &[
                    &game_kind,
                    &Self::STATUS_OPEN,
                    &challenger_id,
                    &target_user_id,
                ],
            )
            .await?;
        Ok(Self::from(row))
    }

    /// Claim an open challenge. Guarded so two simultaneous claims can't both
    /// win: only one UPDATE sees `status = 'open' AND opponent_id IS NULL`.
    /// Generic over the client so it can run inside the claim transaction
    /// that also creates the match chat room.
    pub async fn claim(
        client: &impl GenericClient,
        match_id: Uuid,
        opponent_id: Uuid,
        turn_user_id: Uuid,
        turn_deadline_at: DateTime<Utc>,
        state: &Value,
    ) -> Result<Option<Self>> {
        let row = client
            .query_opt(
                "UPDATE daily_matches
                 SET status = $3,
                     opponent_id = $2,
                     turn_user_id = $4,
                     turn_deadline_at = $5,
                     state = $6,
                     updated = current_timestamp
                 WHERE id = $1
                   AND status = $7
                   AND opponent_id IS NULL
                   AND challenger_id <> $2
                   AND (target_user_id IS NULL OR target_user_id = $2)
                 RETURNING *",
                &[
                    &match_id,
                    &opponent_id,
                    &Self::STATUS_ACTIVE,
                    &turn_user_id,
                    &turn_deadline_at,
                    state,
                    &Self::STATUS_OPEN,
                ],
            )
            .await?;
        Ok(row.map(Self::from))
    }

    /// Attach the match's private chat room, created in the same claim
    /// transaction. Separate from `claim` because the room row needs the
    /// match id for its slug.
    pub async fn set_chat_room(
        client: &impl GenericClient,
        match_id: Uuid,
        chat_room_id: Uuid,
    ) -> Result<u64> {
        let updated = client
            .execute(
                "UPDATE daily_matches
                 SET chat_room_id = $2
                 WHERE id = $1",
                &[&match_id, &chat_room_id],
            )
            .await?;
        Ok(updated)
    }

    pub async fn cancel_challenge(
        client: &Client,
        match_id: Uuid,
        challenger_id: Uuid,
    ) -> Result<u64> {
        let updated = client
            .execute(
                "UPDATE daily_matches
                 SET status = $3,
                     updated = current_timestamp
                 WHERE id = $1
                   AND challenger_id = $2
                   AND status = $4",
                &[
                    &match_id,
                    &challenger_id,
                    &Self::STATUS_CANCELLED,
                    &Self::STATUS_OPEN,
                ],
            )
            .await?;
        Ok(updated)
    }

    /// Persist a played move: new state, turn flipped to `next_turn_user_id`,
    /// a fresh deadline. Applies only while active, only while it is still
    /// `by_user_id`'s turn, and only when the stored state revision still
    /// equals the `expected_revision` the caller loaded. The exact-equality
    /// guard matches `finish`: a battleship hit keeps the turn on the shooter,
    /// so the turn guard alone can't reject a duplicate same-revision write —
    /// only the compare-and-swap makes a superseded move fail loudly instead
    /// of last-write-wins.
    pub async fn update_state(
        client: &Client,
        match_id: Uuid,
        state: &Value,
        by_user_id: Uuid,
        next_turn_user_id: Uuid,
        turn_deadline_at: DateTime<Utc>,
        expected_revision: i64,
    ) -> Result<u64> {
        let updated = client
            .execute(
                &format!(
                    "UPDATE daily_matches
                     SET state = $2,
                         turn_user_id = $4,
                         turn_deadline_at = $5,
                         updated = current_timestamp
                     WHERE id = $1
                       AND status = $6
                       AND turn_user_id = $3
                       AND {}",
                    Self::STORED_REVISION_EQ_SQL
                ),
                &[
                    &match_id,
                    state,
                    &by_user_id,
                    &next_turn_user_id,
                    &turn_deadline_at,
                    &Self::STATUS_ACTIVE,
                    &expected_revision,
                ],
            )
            .await?;
        Ok(updated)
    }

    /// Finish an active match with a final state and result. `winner_user_id`
    /// is NULL for draws. Guarded on `expected_revision` (the stored revision
    /// the caller loaded): if another writer advanced the match in the
    /// meantime the stored revision no longer matches, the update touches 0
    /// rows, and the caller reloads and retries against fresh state instead of
    /// overwriting the concurrent move.
    pub async fn finish(
        client: &Client,
        match_id: Uuid,
        winner_user_id: Option<Uuid>,
        result: &str,
        state: &Value,
        expected_revision: i64,
    ) -> Result<u64> {
        let updated = client
            .execute(
                &format!(
                    "UPDATE daily_matches
                     SET status = $3,
                         winner_user_id = $4,
                         result = $5,
                         state = $2,
                         turn_user_id = NULL,
                         turn_deadline_at = NULL,
                         updated = current_timestamp
                     WHERE id = $1
                       AND status = $6
                       AND {}",
                    Self::STORED_REVISION_EQ_SQL
                ),
                &[
                    &match_id,
                    state,
                    &Self::STATUS_FINISHED,
                    &winner_user_id,
                    &result,
                    &Self::STATUS_ACTIVE,
                    &expected_revision,
                ],
            )
            .await?;
        Ok(updated)
    }

    /// Forfeit every active match whose move deadline has passed. The player
    /// on the clock loses; returns the finished rows so callers can pay out
    /// and broadcast.
    pub async fn forfeit_expired(client: &Client) -> Result<Vec<Self>> {
        let rows = client
            .query(
                "UPDATE daily_matches
                 SET status = $1,
                     result = $2,
                     winner_user_id = CASE
                         WHEN turn_user_id = challenger_id THEN opponent_id
                         ELSE challenger_id
                     END,
                     turn_user_id = NULL,
                     turn_deadline_at = NULL,
                     updated = current_timestamp
                 WHERE status = $3
                   AND turn_deadline_at < current_timestamp
                 RETURNING *",
                &[
                    &Self::STATUS_FINISHED,
                    &Self::RESULT_TIMEOUT,
                    &Self::STATUS_ACTIVE,
                ],
            )
            .await?;
        Ok(rows.into_iter().map(Self::from).collect())
    }

    pub async fn list_open(client: &Client) -> Result<Vec<Self>> {
        let rows = client
            .query(
                "SELECT * FROM daily_matches
                 WHERE status = 'open'
                 ORDER BY created ASC, id ASC",
                &[],
            )
            .await?;
        Ok(rows.into_iter().map(Self::from).collect())
    }

    pub async fn list_active(client: &Client) -> Result<Vec<Self>> {
        let rows = client
            .query(
                "SELECT * FROM daily_matches
                 WHERE status = 'active'
                 ORDER BY turn_deadline_at ASC NULLS LAST, id ASC",
                &[],
            )
            .await?;
        Ok(rows.into_iter().map(Self::from).collect())
    }

    /// Finished matches at least one player hasn't acknowledged yet. The
    /// 30-day window bounds the snapshot when a player never comes back;
    /// `updated` is the finish time (`mark_result_seen` deliberately doesn't
    /// touch it), so old rows age out instead of pinning the list forever.
    pub async fn list_finished_unseen(client: &Client) -> Result<Vec<Self>> {
        let rows = client
            .query(
                "SELECT * FROM daily_matches
                 WHERE status = 'finished'
                   AND (challenger_result_seen_at IS NULL
                        OR (opponent_id IS NOT NULL AND opponent_result_seen_at IS NULL))
                   AND updated > current_timestamp - INTERVAL '30 days'
                 ORDER BY updated DESC, id ASC",
                &[],
            )
            .await?;
        Ok(rows.into_iter().map(Self::from).collect())
    }

    /// One player acknowledges a finished match's result. Touches only the
    /// caller's own seen column, and only while it is still NULL, so a repeat
    /// ack updates 0 rows and the caller can skip republishing. `updated`
    /// stays the finish timestamp (see `list_finished_unseen`).
    pub async fn mark_result_seen(client: &Client, match_id: Uuid, user_id: Uuid) -> Result<u64> {
        let updated = client
            .execute(
                "UPDATE daily_matches
                 SET challenger_result_seen_at = CASE
                         WHEN challenger_id = $2 THEN current_timestamp
                         ELSE challenger_result_seen_at
                     END,
                     opponent_result_seen_at = CASE
                         WHEN opponent_id = $2 THEN current_timestamp
                         ELSE opponent_result_seen_at
                     END
                 WHERE id = $1
                   AND status = $3
                   AND ((challenger_id = $2 AND challenger_result_seen_at IS NULL)
                        OR (opponent_id = $2 AND opponent_result_seen_at IS NULL))",
                &[&match_id, &user_id, &Self::STATUS_FINISHED],
            )
            .await?;
        Ok(updated)
    }

    /// Hard-delete match chat rooms (and their voice channels) once the
    /// match has been finished or cancelled for over 30 days — aligned with
    /// the unseen-result window, so chat outlives every surface that could
    /// still point at it. Chat FK cascades remove memberships/messages;
    /// `daily_matches.chat_room_id` goes NULL via ON DELETE SET NULL, so
    /// match history keeps its row. Voice channels have no FK to their
    /// polymorphic target and must be deleted alongside, mirroring the
    /// rooms cleanup CTE.
    pub async fn delete_stale_chat_rooms(client: &Client) -> Result<u64> {
        let row = client
            .query_one(
                "WITH stale AS (
                     SELECT chat_room_id AS id
                     FROM daily_matches
                     WHERE chat_room_id IS NOT NULL
                       AND status IN ($1, $2)
                       AND updated < current_timestamp - INTERVAL '30 days'
                 ),
                 deleted_voice AS (
                     DELETE FROM voice_channels v
                     USING stale s
                     WHERE v.target_kind = 'chat_room'
                       AND v.target_id = s.id
                     RETURNING v.id
                 ),
                 deleted_chats AS (
                     DELETE FROM chat_rooms c
                     USING stale s
                     WHERE c.id = s.id
                     RETURNING c.id
                 )
                 SELECT COUNT(*)::bigint AS count FROM deleted_chats",
                &[&Self::STATUS_FINISHED, &Self::STATUS_CANCELLED],
            )
            .await?;
        let count: i64 = row.get("count");
        Ok(count as u64)
    }

    /// Optimistic compare-and-swap guard shared by `update_state` and
    /// `finish`: apply only when the stored `state.revision` still equals the
    /// `$7` revision the caller loaded, so a concurrent move (which advances
    /// the revision) makes the write a no-op instead of clobbering it.
    const STORED_REVISION_EQ_SQL: &'static str = "(
        COALESCE(
          CASE
            WHEN state ? 'revision'
             AND state->>'revision' ~ '^[0-9]+$'
            THEN (state->>'revision')::bigint
            ELSE 0
          END,
          0
        )
        = $7
    )";
}
