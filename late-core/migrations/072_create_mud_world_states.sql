-- Persistent global state for Lateania worlds. Character saves remain per-user
-- in mud_characters; this table stores the shared world/runtime slice such as
-- mob health and respawn timers.
CREATE TABLE mud_world_states (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    world_key TEXT NOT NULL UNIQUE,
    data JSONB NOT NULL DEFAULT '{}'::jsonb
);
