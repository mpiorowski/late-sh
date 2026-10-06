-- Traffic keeps one best score per (user, track); the Traffic high score is the
-- sum of a user's per-track bests. Per-track scores are normalized 0..1000.
CREATE TABLE traffic_track_scores (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    track_key TEXT NOT NULL,
    score INT NOT NULL DEFAULT 0,
    UNIQUE (user_id, track_key)
);

-- Aggregate row mirroring the other high-score games so leaderboard queries
-- stay uniform. score is always the sum of traffic_track_scores for the user.
CREATE TABLE traffic_high_scores (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE UNIQUE,
    score INT NOT NULL DEFAULT 0
);
