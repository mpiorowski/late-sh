use anyhow::Result;
use chrono::{DateTime, Utc};
use deadpool_postgres::GenericClient;
use serde_json::Value;
use tokio_postgres::{Client, error::SqlState};
use uuid::Uuid;

/// The notify channel a row-level trigger (migration 241) fires on every
/// write to `realm_games`. Every replica re-reads its realm snapshot on it;
/// the payload is empty and never trusted.
pub const REALM_GAME_CHANGED_CHANNEL: &str = "realm_game_changed";

// One realm game: a season of the house's standing world. `season` numbers
// the worlds in order (0 on rows from before the house opened them);
// `name` is the label player-made realms carried, blank on house worlds
// (read as "Season N"); `reset_hour_utc` (0-23) is when its action
// points refill; `last_resolved_day` is the last realm day the sweeper
// closed out (log archived, idle clocks checked); `payout` is what the pot
// did once a finished game was settled (`paid`, `unearned`, `dissolved`),
// NULL until it has been. At most one row is open or active at a time
// (`realm_games_one_live`).
crate::model! {
    table = "realm_games";
    params = RealmGameParams;
    struct RealmGame {
        @data
        pub status: String,
        pub season: i32,
        pub creator_id: Uuid,
        pub name: String,
        pub ruleset_id: String,
        pub chat_room_id: Option<Uuid>,
        pub reset_hour_utc: i16,
        pub last_resolved_day: i32,
        pub winner_user_id: Option<Uuid>,
        pub payout: Option<String>,
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
    /// The board the day's night opened on, before any of its fights landed:
    /// what the replay draws under them. `None` on instant games, on days
    /// archived before it was kept, and on a day still being played.
    pub board_open: Option<Value>,
}

impl From<tokio_postgres::Row> for RealmDay {
    fn from(row: tokio_postgres::Row) -> Self {
        Self {
            game_id: row.get("game_id"),
            day: row.get("day"),
            seed: row.get("seed"),
            board: row.get("board"),
            board_open: row.get("board_open"),
            log: row.get("log"),
        }
    }
}

impl RealmGame {
    pub const STATUS_OPEN: &'static str = "open";
    pub const STATUS_ACTIVE: &'static str = "active";
    pub const STATUS_FINISHED: &'static str = "finished";
    pub const STATUS_CANCELLED: &'static str = "cancelled";

    /// `payout` spellings; the column's CHECK is the closed set.
    pub const PAYOUT_PAID: &'static str = "paid";
    pub const PAYOUT_UNEARNED: &'static str = "unearned";
    pub const PAYOUT_DISSOLVED: &'static str = "dissolved";

    /// Insert the next world, already live: the row lands active, so there
    /// is no second write to promote it and nothing between two writes that
    /// can strand it. The caller holds the opening claim
    /// (`try_lock_season_opening`) and passes the season it read under it.
    /// `None` when a world is already standing: the partial unique index
    /// `realm_games_one_live` refuses a second open or active row, whatever
    /// path the write came by.
    pub async fn open_season(
        client: &impl GenericClient,
        season: i32,
        creator_id: Uuid,
        ruleset_id: &str,
        reset_hour_utc: i16,
        last_resolved_day: i32,
        state: &Value,
    ) -> Result<Option<Self>> {
        let inserted = client
            .query_one(
                "INSERT INTO realm_games
                     (status, season, creator_id, ruleset_id, reset_hour_utc,
                      last_resolved_day, state)
                 VALUES ($1, $2, $3, $4, $5, $6, $7)
                 RETURNING *",
                &[
                    &Self::STATUS_ACTIVE,
                    &season,
                    &creator_id,
                    &ruleset_id,
                    &reset_hour_utc,
                    &last_resolved_day,
                    state,
                ],
            )
            .await;
        match inserted {
            Ok(row) => Ok(Some(Self::from(row))),
            Err(error) if error.code() == Some(&SqlState::UNIQUE_VIOLATION) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    /// Claim the opening of the next season, for the length of the caller's
    /// transaction. `false` when another replica holds it: that one is
    /// opening the world, and this one spends nothing on it (CONTEXT.md,
    /// once-only actions). Released when the transaction ends, committed or
    /// not, so an opening that dies half way leaves no claim behind.
    pub async fn try_lock_season_opening(client: &impl GenericClient) -> Result<bool> {
        let row = client
            .query_one(
                "SELECT pg_try_advisory_xact_lock(hashtextextended('realm_season_opening', 0))
                     AS claimed",
                &[],
            )
            .await?;
        Ok(row.get("claimed"))
    }

    /// The number the next world will carry: one past the highest so far.
    /// Read under the opening claim, so it is handed out once.
    pub async fn next_season(client: &impl GenericClient) -> Result<i32> {
        let row = client
            .query_one(
                "SELECT (COALESCE(MAX(season), 0) + 1)::int AS season FROM realm_games",
                &[],
            )
            .await?;
        Ok(row.get("season"))
    }

    /// The standing world: the one open or active row, if there is one.
    pub async fn live(client: &impl GenericClient) -> Result<Option<Self>> {
        let row = client
            .query_opt(
                "SELECT * FROM realm_games
                 WHERE status IN ('open', 'active')
                 ORDER BY created DESC, id DESC
                 LIMIT 1",
                &[],
            )
            .await?;
        Ok(row.map(Self::from))
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
    /// Games that finished within the last `hours` — what the hub card and
    /// the live strip still show so the result can be read. The rows live on either way; this is
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

    /// Active games whose day cursor trails their own clock at `now`, oldest
    /// first — the sweeper's work list. Each game keeps its own
    /// `reset_hour_utc`, so "which realm day is it" is computed per row, and
    /// it is computed here rather than after the fetch: the per-tick `limit`
    /// has to land on games with work to do, or twenty up-to-date games at
    /// the front of the list would hide every game behind them for good.
    /// The day arithmetic is the SQL spelling of the service's `realm_day`.
    pub async fn list_active_for_sweep(
        client: &Client,
        now: DateTime<Utc>,
        limit: i64,
    ) -> Result<Vec<Self>> {
        let rows = client
            .query(
                "SELECT * FROM realm_games
                 WHERE status = 'active'
                   AND last_resolved_day <
                       floor((extract(epoch FROM $1::timestamptz) - reset_hour_utc * 3600) / 86400)::int
                 ORDER BY created ASC, id ASC
                 LIMIT $2",
                &[&now, &limit],
            )
            .await?;
        Ok(rows.into_iter().map(Self::from).collect())
    }

    /// Finished games whose pot has not been settled, oldest first — the
    /// sweeper's other work list. A finish is durable before its chips move,
    /// so anything that stopped the credits (a crash, a failed write) leaves
    /// a row here until a later tick settles it.
    pub async fn list_finished_unpaid(client: &Client, limit: i64) -> Result<Vec<Self>> {
        let rows = client
            .query(
                "SELECT * FROM realm_games
                 WHERE status = 'finished'
                   AND payout IS NULL
                 ORDER BY updated ASC, id ASC
                 LIMIT $1",
                &[&limit],
            )
            .await?;
        Ok(rows.into_iter().map(Self::from).collect())
    }

    /// Record what the pot did, once. Only a finished game that has not been
    /// settled takes the write, so two replicas settling the same game both
    /// credit (idempotently, through the payout claims) and one records.
    pub async fn set_payout(client: &Client, game_id: Uuid, payout: &str) -> Result<u64> {
        let updated = client
            .execute(
                "UPDATE realm_games
                 SET payout = $2
                 WHERE id = $1
                   AND status = 'finished'
                   AND payout IS NULL",
                &[&game_id, &payout],
            )
            .await?;
        Ok(updated)
    }

    /// Replace the state JSONB while the game is open or active. Guarded by
    /// the stored-revision compare-and-swap: a concurrent writer advanced the
    /// revision, this touches 0 rows, and the caller reloads instead of
    /// clobbering. Used by join and leave.
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

    /// Give a world its chat room (`ChatRoom::get_or_create_realm_room`, one
    /// per world, so a new season starts on an empty room) and record it on
    /// the row. Idempotent: a second call lands on the same room.
    pub async fn attach_chat_room(client: &impl GenericClient, game_id: Uuid) -> Result<Uuid> {
        let room = super::chat_room::ChatRoom::get_or_create_realm_room(client, game_id).await?;
        Self::set_chat_room(client, game_id, room.id).await?;
        Ok(room.id)
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

    /// Hard-delete the chat rooms (and their voice channels) of worlds that
    /// finished or were cancelled over 30 days ago, long after the 24h the
    /// board stays listed, so the talk outlives every surface that could
    /// show it. Chat FK cascades remove memberships and messages;
    /// `realm_games.chat_room_id` goes NULL via ON DELETE SET NULL, so the
    /// world keeps its row. Voice channels have no FK to their polymorphic
    /// target and go alongside, the `DailyMatch::delete_stale_chat_rooms`
    /// shape.
    pub async fn delete_stale_chat_rooms(client: &impl GenericClient) -> Result<u64> {
        let row = client
            .query_one(
                "WITH stale AS (
                     SELECT chat_room_id AS id
                     FROM realm_games
                     WHERE chat_room_id IS NOT NULL
                       AND status IN ('finished', 'cancelled')
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
                &[],
            )
            .await?;
        let count: i64 = row.get("count");
        Ok(count as u64)
    }

    /// Archive one day's log. Re-archiving the same day overwrites it: a day
    /// is written when it rolls over, but a crash mid-day can leave a partial
    /// row that the next write completes.
    /// Write a day's log, and optionally the board as that day closed and
    /// the board its night opened on.
    ///
    /// The boards are `None` for the live upserts that happen while a day is
    /// being played, and `Some` once at the rollover (and again when the game
    /// ends). The `COALESCE` on each is what keeps those apart: a mid-day
    /// write must never replace a snapshot with a half-finished one, and the
    /// day row is rewritten on every action.
    pub async fn archive_day(
        client: &Client,
        game_id: Uuid,
        day: i32,
        seed: i64,
        log: &Value,
        board: Option<&Value>,
        board_open: Option<&Value>,
    ) -> Result<u64> {
        let inserted = client
            .execute(
                "INSERT INTO realm_days (game_id, day, seed, log, board, board_open)
                 VALUES ($1, $2, $3, $4, $5, $6)
                 ON CONFLICT (game_id, day) DO UPDATE
                 SET seed = EXCLUDED.seed,
                     log = EXCLUDED.log,
                     board = COALESCE(EXCLUDED.board, realm_days.board),
                     board_open = COALESCE(EXCLUDED.board_open, realm_days.board_open)",
                &[&game_id, &day, &seed, log, &board, &board_open],
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
