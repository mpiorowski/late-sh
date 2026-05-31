CREATE TABLE user_daily_quest_streaks (
    user_id UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    last_completed_date DATE NOT NULL,
    consecutive_days INT NOT NULL CHECK (consecutive_days > 0),
    bonus_level INT NOT NULL DEFAULT 0 CHECK (bonus_level BETWEEN 0 AND 5)
);

CREATE INDEX user_daily_quest_streaks_last_completed_idx
    ON user_daily_quest_streaks (last_completed_date DESC);
