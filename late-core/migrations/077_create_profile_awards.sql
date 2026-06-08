CREATE TABLE profile_awards (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    category TEXT NOT NULL,
    period_month DATE NOT NULL,
    rank INT NOT NULL CHECK (rank >= 1),
    score_value BIGINT NOT NULL,
    awarded_at TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    UNIQUE (user_id, category, period_month)
);

CREATE INDEX profile_awards_user_period_idx
    ON profile_awards (user_id, period_month DESC, rank ASC);

CREATE INDEX profile_awards_current_lookup_idx
    ON profile_awards (period_month DESC, user_id, rank ASC);
