-- One public arcade handle per account: the human-readable, immutable name a
-- user carries into door games whose upstream binaries key saves and public
-- score files by player name (DCSS today; NetHack may adopt it later).
-- Claimed once, never changed, unique case-insensitively.
--
-- user_id is ON DELETE SET NULL (not the usual CASCADE) on purpose: the row
-- must outlive the account as a graveyard entry. Door saves and public
-- logfiles on the game hosts are keyed by the handle text, so freeing a
-- deleted account's handle would let its next claimant open the dead
-- account's saved games.
CREATE TABLE arcade_handles (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    user_id UUID UNIQUE REFERENCES users(id) ON DELETE SET NULL,
    handle TEXT NOT NULL
);

CREATE UNIQUE INDEX arcade_handles_handle_lower_idx ON arcade_handles (lower(handle));
