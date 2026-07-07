-- Per-user tavern drink tally for the clubhouse @bartender. drunk_points is
-- the raw buzz at last_drink_at (chips spent on drinks, capped); the effective
-- level is computed at read time by decaying against elapsed time, so no
-- background sober-up task exists. lifetime_spent/drink_count are permanent
-- stats for future leaderboards.
CREATE TABLE user_drinks (
    user_id UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    drunk_points BIGINT NOT NULL DEFAULT 0 CHECK (drunk_points >= 0),
    lifetime_spent BIGINT NOT NULL DEFAULT 0 CHECK (lifetime_spent >= 0),
    drink_count BIGINT NOT NULL DEFAULT 0 CHECK (drink_count >= 0),
    last_drink_at TIMESTAMPTZ NOT NULL DEFAULT current_timestamp
);

CREATE INDEX user_drinks_last_drink_idx ON user_drinks (last_drink_at DESC);
