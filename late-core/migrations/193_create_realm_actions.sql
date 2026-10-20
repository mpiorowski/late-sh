-- Per-player hidden action queues for realm games: one row per
-- (game, player, UTC day). Each player CAS-writes only their own row via the
-- revision column, so concurrent players never contend and two sessions of the
-- same player resolve last-write-wins through the CAS.
CREATE TABLE realm_actions (
    game_id UUID NOT NULL REFERENCES realm_games(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    day INTEGER NOT NULL,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    revision BIGINT NOT NULL DEFAULT 1,
    queue JSONB NOT NULL DEFAULT '[]'::jsonb,
    PRIMARY KEY (game_id, user_id, day)
);

-- Archived per-day resolution logs so the realm_games state JSONB keeps only
-- the latest day and week-long games don't grow the hot row unboundedly.
CREATE TABLE realm_days (
    game_id UUID NOT NULL REFERENCES realm_games(id) ON DELETE CASCADE,
    day INTEGER NOT NULL,
    seed BIGINT NOT NULL,
    log JSONB NOT NULL,
    PRIMARY KEY (game_id, day)
);
