CREATE TABLE nonogram_games (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    mode VARCHAR NOT NULL,
    difficulty_key VARCHAR NOT NULL,
    puzzle_date DATE,
    puzzle_id VARCHAR NOT NULL,
    player_grid JSONB NOT NULL,
    is_game_over BOOLEAN NOT NULL DEFAULT false,
    score INT NOT NULL DEFAULT 0,
    UNIQUE(user_id, difficulty_key, mode)
);

CREATE TABLE nonogram_daily_wins (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    difficulty_key VARCHAR NOT NULL,
    puzzle_date DATE NOT NULL,
    UNIQUE(user_id, difficulty_key, puzzle_date)
);
