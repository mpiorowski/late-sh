-- Chess Puzzle's dailies are claimed like Quartets' wall: the first session
-- of a UTC day inserts the date's row per tier under an advisory lock,
-- never-used puzzles first. puzzle_id names a pack entry of that tier. A
-- puzzle may come round again once its tier's pack is used up, so
-- puzzle_id is not unique here.
CREATE TABLE chess_puzzle_daily_puzzles (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    puzzle_date DATE NOT NULL,
    difficulty_key VARCHAR NOT NULL CHECK (difficulty_key IN ('easy', 'medium', 'hard')),
    puzzle_id VARCHAR NOT NULL,
    UNIQUE(puzzle_date, difficulty_key)
);
