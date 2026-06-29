CREATE TABLE solitaire_games (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    mode VARCHAR NOT NULL,
    difficulty_key VARCHAR NOT NULL,
    puzzle_date DATE,
    puzzle_seed BIGINT NOT NULL,
    stock JSONB NOT NULL,
    waste JSONB NOT NULL,
    foundations JSONB NOT NULL,
    tableau JSONB NOT NULL,
    is_game_over BOOLEAN NOT NULL DEFAULT false,
    score INT NOT NULL DEFAULT 0,
    UNIQUE(user_id, difficulty_key, mode)
);

CREATE TABLE solitaire_daily_wins (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    difficulty_key VARCHAR NOT NULL,
    puzzle_date DATE NOT NULL,
    score INT NOT NULL DEFAULT 0,
    UNIQUE(user_id, difficulty_key, puzzle_date)
);
