CREATE TABLE bonsai_trees (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE UNIQUE,
    growth_points INT NOT NULL DEFAULT 0,
    last_watered DATE,
    seed BIGINT NOT NULL,
    is_alive BOOLEAN NOT NULL DEFAULT true
);

CREATE TABLE bonsai_graveyard (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    survived_days INT NOT NULL,
    died_at TIMESTAMPTZ NOT NULL DEFAULT current_timestamp
);
