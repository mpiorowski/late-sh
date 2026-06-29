CREATE TABLE twenty_forty_eight_games (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE UNIQUE,
    score INT NOT NULL DEFAULT 0,
    grid JSONB NOT NULL DEFAULT '[[0,0,0,0],[0,0,0,0],[0,0,0,0],[0,0,0,0]]'::jsonb,
    is_game_over BOOLEAN NOT NULL DEFAULT false
);

CREATE TABLE twenty_forty_eight_high_scores (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE UNIQUE,
    score INT NOT NULL DEFAULT 0
);
