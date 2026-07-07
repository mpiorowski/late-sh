-- Persistent Legend of the Green Dragon characters. One character per user,
-- stored as a schema-versioned JSON blob so the game can evolve its character
-- shape without a migration per field. The game owns the blob's contents; the
-- table only guarantees one row per user and tracks when it was last saved.
-- Mirrors mud_characters (the Lateania door) exactly.
CREATE TABLE greendragon_characters (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE UNIQUE,
    data JSONB NOT NULL DEFAULT '{}'::jsonb
);
