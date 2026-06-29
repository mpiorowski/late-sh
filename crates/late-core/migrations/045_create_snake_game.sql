CREATE TABLE snake_games (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE UNIQUE,
    score INT NOT NULL DEFAULT 0,
    level INT NOT NULL DEFAULT 1,
    lives INT NOT NULL DEFAULT 3,
    is_game_over BOOLEAN NOT NULL DEFAULT false
);

CREATE TABLE snake_high_scores (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE UNIQUE,
    score INT NOT NULL DEFAULT 0
);
