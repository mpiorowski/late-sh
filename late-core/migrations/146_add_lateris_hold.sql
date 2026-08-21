-- The hold slot: the piece parked out of play, and whether this piece has
-- already used its one hold. Both nullable/defaulted so games saved before the
-- feature restore as "nothing held, hold available".
ALTER TABLE tetris_games
    ADD COLUMN hold_kind TEXT,
    ADD COLUMN hold_used BOOLEAN NOT NULL DEFAULT false;
