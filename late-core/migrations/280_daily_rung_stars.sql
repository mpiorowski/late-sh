-- The rung: one daily board a day, the same for everyone, keyed to the
-- calendar (late-core/src/models/daily_rung.rs). A star is that board won
-- on its day, one a day; the count is the standing it gives.
CREATE TABLE daily_rung_stars (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    day DATE NOT NULL,
    puzzle TEXT NOT NULL,
    difficulty_key TEXT NOT NULL,
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    PRIMARY KEY (user_id, day)
);

CREATE INDEX daily_rung_stars_day_idx ON daily_rung_stars (day);
