CREATE TABLE bonsai_daily_care (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    care_date DATE NOT NULL,
    watered BOOLEAN NOT NULL DEFAULT false,
    cut_branch_ids INT[] NOT NULL DEFAULT '{}',
    branch_goal INT NOT NULL,
    water_penalty_applied BOOLEAN NOT NULL DEFAULT false,
    prune_penalty_applied BOOLEAN NOT NULL DEFAULT false,
    UNIQUE(user_id, care_date)
);

CREATE INDEX idx_bonsai_daily_care_user_date
    ON bonsai_daily_care(user_id, care_date DESC);
