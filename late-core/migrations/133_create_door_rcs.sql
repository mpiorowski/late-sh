-- Per-account config files for the roguelike doors: NetHack's .nethackrc and
-- DCSS's init.txt. The DB row is the source of truth; the door client pushes
-- the content to the game host on every launch (one SSH env request), where
-- it lands as an ephemeral per-player file the child reads. ON DELETE CASCADE
-- (unlike arcade_handles' graveyard rows): an rc is worthless without its
-- account and nothing on the hosts is keyed by it.
CREATE TABLE door_rcs (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    game TEXT NOT NULL,
    content TEXT NOT NULL,
    UNIQUE (user_id, game)
);
