CREATE TABLE rubiks_cube_daily_wins (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    puzzle_date DATE NOT NULL,
    UNIQUE(user_id, puzzle_date)
);

