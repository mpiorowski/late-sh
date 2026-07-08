use anyhow::Result;
use chrono::{DateTime, Utc};
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

    pub const GAME_KIND_CHESS: &'static str = "chess";

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
        challenger_id: Uuid,
        target_user_id: Option<Uuid>,
    ) -> Result<Self> {
        let row = client
            .query_one(
                "INSERT INTO daily_matches (game_kind, status, challenger_id, target_user_id)
                 VALUES ($1, $2, $3, $4)
                 RETURNING *",
                &[
                    &Self::GAME_KIND_CHESS,
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
    pub async fn claim(
        client: &Client,
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
    /// `by_user_id`'s turn (so a duplicate in-flight move can't double-apply),
    /// and only when the stored state revision is not ahead of the incoming
    /// one (same monotonic guard as `GameRoom::update_runtime_state`).
    pub async fn update_state(
        client: &Client,
        match_id: Uuid,
        state: &Value,
        by_user_id: Uuid,
        next_turn_user_id: Uuid,
        turn_deadline_at: DateTime<Utc>,
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
                    Self::REVISION_GUARD_SQL
                ),
                &[
                    &match_id,
                    state,
                    &by_user_id,
                    &next_turn_user_id,
                    &turn_deadline_at,
                    &Self::STATUS_ACTIVE,
                ],
            )
            .await?;
        Ok(updated)
    }

    /// Finish an active match with a final state and result. `winner_user_id`
    /// is NULL for draws.
    pub async fn finish(
        client: &Client,
        match_id: Uuid,
        winner_user_id: Option<Uuid>,
        result: &str,
        state: &Value,
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
                    Self::REVISION_GUARD_SQL
                ),
                &[
                    &match_id,
                    state,
                    &Self::STATUS_FINISHED,
                    &winner_user_id,
                    &result,
                    &Self::STATUS_ACTIVE,
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

    /// Monotonic revision guard shared by the mutating state writes: apply
    /// only when the stored `state.revision` is <= the incoming `$2` state's.
    const REVISION_GUARD_SQL: &'static str = "(
        COALESCE(
          CASE
            WHEN state ? 'revision'
             AND state->>'revision' ~ '^[0-9]+$'
            THEN (state->>'revision')::bigint
            ELSE 0
          END,
          0
        )
        <=
        COALESCE(
          CASE
            WHEN ($2::jsonb) ? 'revision'
             AND ($2::jsonb)->>'revision' ~ '^[0-9]+$'
            THEN (($2::jsonb)->>'revision')::bigint
            ELSE 0
          END,
          0
        )
    )";
}
