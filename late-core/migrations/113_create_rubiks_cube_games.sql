-- Persist the Rubik's Cube daily board so leaving the app no longer loses
-- cube progress. One row per user: the daily scramble is deterministic per
-- date, so stale rows (puzzle_date != today) are ignored on load and
-- overwritten by the next move. Stickers are a 54-char face string
-- (U D L R F B faces, 9 stickers each, W/Y/O/R/G/B per sticker).
CREATE TABLE rubiks_cube_games (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    puzzle_date DATE NOT NULL,
    stickers VARCHAR NOT NULL,
    user_moves INT NOT NULL DEFAULT 0,
    UNIQUE(user_id)
);
