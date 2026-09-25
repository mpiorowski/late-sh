use anyhow::Result;
use serde_json::Value;
use tokio_postgres::Client;
use uuid::Uuid;

// One realm game. `name` is the creator's label for it (empty means the
// service fills in "<creator>'s realm"); `reset_hour_utc` (0-23) is when this game's action points
// refill, picked at creation so a game can run on its players' clock rather
// than a global midnight; `last_resolved_day` is the last realm day the
// sweeper closed out (log archived, idle clocks checked).
crate::model! {
    table = "realm_games";
    params = RealmGameParams;
    struct RealmGame {
        @data
        pub status: String,
        pub creator_id: Uuid,
        pub name: String,
        pub ruleset_id: String,
        pub chat_room_id: Option<Uuid>,
        pub reset_hour_utc: i16,
        pub last_resolved_day: i32,
        pub winner_user_id: Option<Uuid>,
        pub state: Value,
    }
}

/// One archived resolved day of one game.
#[derive(Debug, Clone)]
pub struct RealmDay {
    pub game_id: Uuid,
    pub day: i32,
    pub seed: i64,
    pub log: Value,
    /// The board as this day closed. `None` on days archived before boards
    /// were kept, and on a day still being played.
    pub board: Option<Value>,
}

impl From<tokio_postgres::Row> for RealmDay {
    fn from(row: tokio_postgres::Row) -> Self {
        Self {
            game_id: row.get("game_id"),
            day: row.get("day"),
            seed: row.get("seed"),
            board: row.get("board"),
            log: row.get("log"),
        }
    }
}

impl RealmGame {
    pub const STATUS_OPEN: &'static str = "open";
    pub const STATUS_ACTIVE: &'static str = "active";
    pub const STATUS_FINISHED: &'static str = "finished";
    pub const STATUS_CANCELLED: &'static str = "cancelled";

    pub async fn create_game(
        client: &Client,
        creator_id: Uuid,
        name: &str,
        ruleset_id: &str,
        reset_hour_utc: i16,
        last_resolved_day: i32,
        state: &Value,
    ) -> Result<Self> {
        let row = client
            .query_one(
                "INSERT INTO realm_games
                     (status, creator_id, name, ruleset_id, reset_hour_utc,
                      last_resolved_day, state)
                 VALUES ($1, $2, $3, $4, $5, $6, $7)
                 RETURNING *",
                &[
                    &Self::STATUS_OPEN,
                    &creator_id,
                    &name,
                    &ruleset_id,
                    &reset_hour_utc,
                    &last_resolved_day,
                    state,
                ],
            )
            .await?;
        Ok(Self::from(row))
    }

    /// Open or active games the user is a member of (creator or joined) —
    /// the entry-cap input. Membership lives in the state JSONB's players
    /// array; `@>` with a partial object matches on user_id alone.
    pub async fn count_active_memberships(client: &Client, user_id: Uuid) -> Result<i64> {
        let row = client
            .query_one(
                "SELECT COUNT(*)::bigint AS count
                 FROM realm_games
                 WHERE status IN ('open', 'active')
                   AND state->'players' @> jsonb_build_array(jsonb_build_object('user_id', $1::uuid))",
                &[&user_id],
            )
            .await?;
        let count: i64 = row.get("count");
        Ok(count)
    }

    pub async fn list_open(client: &Client) -> Result<Vec<Self>> {
        let rows = client
            .query(
                "SELECT * FROM realm_games
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
                "SELECT * FROM realm_games
                 WHERE status = 'active'
                 ORDER BY created ASC, id ASC",
                &[],
            )
            .await?;
        Ok(rows.into_iter().map(Self::from).collect())
    }

    /// Recently finished games, bounded like the daily unseen-result window so
    /// the snapshot can show fresh results without pinning rows forever.
    /// Games that finished within the last `hours` — what the Lobby still
    /// shows so the result can be read. The rows live on either way; this is
    /// only about what is worth listing.
    pub async fn list_finished_recent(client: &Client, hours: i64) -> Result<Vec<Self>> {
        let rows = client
            .query(
                "SELECT * FROM realm_games
                 WHERE status = 'finished'
                   AND updated > current_timestamp - make_interval(hours => $1::int)
                 ORDER BY updated DESC, id ASC",
                &[&(hours as i32)],
            )
            .await?;
        Ok(rows.into_iter().map(Self::from).collect())
    }

    /// Every active game, oldest first — the sweeper's work list. The day
    /// cursor is compared in Rust rather than SQL because each game keeps its
    /// own `reset_hour_utc`, so "which day is it" is per row.
    pub async fn list_active_for_sweep(client: &Client, limit: i64) -> Result<Vec<Self>> {
        let rows = client
            .query(
                "SELECT * FROM realm_games
                 WHERE status = 'active'
                 ORDER BY created ASC, id ASC
                 LIMIT $1",
                &[&limit],
            )
            .await?;
        Ok(rows.into_iter().map(Self::from).collect())
    }

    /// Replace the state JSONB while the game is open or active. Guarded by
    /// the stored-revision compare-and-swap: a concurrent writer advanced the
    /// revision, this touches 0 rows, and the caller reloads instead of
    /// clobbering. Used by join/leave and any pre-start mutation.
    pub async fn update_state_cas(
        client: &Client,
        game_id: Uuid,
        state: &Value,
        expected_revision: i64,
    ) -> Result<u64> {
        let updated = client
            .execute(
                &format!(
                    "UPDATE realm_games
                     SET state = $2,
                         updated = current_timestamp
                     WHERE id = $1
                       AND status IN ('open', 'active')
                       AND {}",
                    Self::stored_revision_eq_sql("$3")
                ),
                &[&game_id, state, &expected_revision],
            )
            .await?;
        Ok(updated)
    }

    /// Start an open game: flip to active, set the resolution day floor, and
    /// persist the spawn-populated state. Creator-guarded and CAS-guarded.
    pub async fn start_cas(
        client: &Client,
        game_id: Uuid,
        creator_id: Uuid,
        last_resolved_day: i32,
        state: &Value,
        expected_revision: i64,
    ) -> Result<u64> {
        let updated = client
            .execute(
                &format!(
                    "UPDATE realm_games
                     SET status = 'active',
                         last_resolved_day = $3,
                         state = $2,
                         updated = current_timestamp
                     WHERE id = $1
                       AND status = 'open'
                       AND creator_id = $4
                       AND {}",
                    Self::stored_revision_eq_sql("$5")
                ),
                &[
                    &game_id,
                    state,
                    &last_resolved_day,
                    &creator_id,
                    &expected_revision,
                ],
            )
            .await?;
        Ok(updated)
    }

    /// Persist an action or a sweep: new state, and the day cursor moved to
    /// the game's current realm day. CAS guarded, so the loser of a race
    /// reloads and acts against the world as it actually is.
    pub async fn advance_day_cas(
        client: &Client,
        game_id: Uuid,
        day: i32,
        state: &Value,
        expected_revision: i64,
    ) -> Result<u64> {
        let updated = client
            .execute(
                &format!(
                    "UPDATE realm_games
                     SET state = $2,
                         last_resolved_day = GREATEST(last_resolved_day, $3),
                         updated = current_timestamp
                     WHERE id = $1
                       AND status = 'active'
                       AND {}",
                    Self::stored_revision_eq_sql("$4")
                ),
                &[&game_id, state, &day, &expected_revision],
            )
            .await?;
        Ok(updated)
    }

    /// Finish an active game (win detected by the resolver, or last player
    /// left). `winner_user_id` is None when a game dissolves without a winner.
    pub async fn finish_cas(
        client: &Client,
        game_id: Uuid,
        resolved_day: i32,
        winner_user_id: Option<Uuid>,
        state: &Value,
        expected_revision: i64,
    ) -> Result<u64> {
        let updated = client
            .execute(
                &format!(
                    "UPDATE realm_games
                     SET status = 'finished',
                         winner_user_id = $4,
                         last_resolved_day = $3,
                         state = $2,
                         updated = current_timestamp
                     WHERE id = $1
                       AND status = 'active'
                       AND {}",
                    Self::stored_revision_eq_sql("$5")
                ),
                &[
                    &game_id,
                    state,
                    &resolved_day,
                    &winner_user_id,
                    &expected_revision,
                ],
            )
            .await?;
        Ok(updated)
    }

    /// Pack a realm away. Only ever called for a game with a single player
    /// walking out of it — there is nobody to hand it to and nothing to
    /// finish — so the roster check lives in the service, which has the
    /// state loaded; this guards only the status.
    pub async fn cancel(client: &Client, game_id: Uuid) -> Result<u64> {
        let updated = client
            .execute(
                "UPDATE realm_games
                 SET status = 'cancelled',
                     updated = current_timestamp
                 WHERE id = $1
                   AND status IN ('open', 'active')",
                &[&game_id],
            )
            .await?;
        Ok(updated)
    }

    pub async fn set_chat_room(
        client: &impl deadpool_postgres::GenericClient,
        game_id: Uuid,
        chat_room_id: Uuid,
    ) -> Result<u64> {
        let updated = client
            .execute(
                "UPDATE realm_games
                 SET chat_room_id = $2
                 WHERE id = $1",
                &[&game_id, &chat_room_id],
            )
            .await?;
        Ok(updated)
    }

    /// Archive one day's log. Re-archiving the same day overwrites it: a day
    /// is written when it rolls over, but a crash mid-day can leave a partial
    /// row that the next write completes.
    /// Write a day's log, and optionally the board as that day closed.
    ///
    /// `board` is `None` for the live upserts that happen while a day is
    /// being played, and `Some` once at the rollover (and again when the game
    /// ends). The `COALESCE` is what keeps those apart: a mid-day write must
    /// never replace the closing snapshot with a half-finished one, and the
    /// day row is rewritten on every action.
    pub async fn archive_day(
        client: &Client,
        game_id: Uuid,
        day: i32,
        seed: i64,
        log: &Value,
        board: Option<&Value>,
    ) -> Result<u64> {
        let inserted = client
            .execute(
                "INSERT INTO realm_days (game_id, day, seed, log, board)
                 VALUES ($1, $2, $3, $4, $5)
                 ON CONFLICT (game_id, day) DO UPDATE
                 SET seed = EXCLUDED.seed,
                     log = EXCLUDED.log,
                     board = COALESCE(EXCLUDED.board, realm_days.board)",
                &[&game_id, &day, &seed, log, &board],
            )
            .await?;
        Ok(inserted)
    }

    /// Resolved-day logs newest-first, paged by `before_day` for the log view.
    pub async fn load_day_logs(
        client: &Client,
        game_id: Uuid,
        before_day: Option<i32>,
        limit: i64,
    ) -> Result<Vec<RealmDay>> {
        let rows = client
            .query(
                "SELECT * FROM realm_days
                 WHERE game_id = $1
                   AND ($2::int IS NULL OR day < $2)
                 ORDER BY day DESC
                 LIMIT $3",
                &[&game_id, &before_day, &limit],
            )
            .await?;
        Ok(rows.into_iter().map(RealmDay::from).collect())
    }

    /// Optimistic compare-and-swap guard (daily_match.rs pattern, with the
    /// placeholder index passed in because these queries bind different arity).
    fn stored_revision_eq_sql(param: &str) -> String {
        format!(
            "(
                COALESCE(
                  CASE
                    WHEN state ? 'revision'
                     AND state->>'revision' ~ '^[0-9]+$'
                    THEN (state->>'revision')::bigint
                    ELSE 0
                  END,
                  0
                )
                = {param}
            )"
        )
    }
}
