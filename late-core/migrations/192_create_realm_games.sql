-- Realm: multiplayer simultaneous-turn territory conquest. One row per game.
-- Cold state (ruleset snapshot, players, ownership, last day log) lives in the
-- state JSONB with a top-level revision guarded by compare-and-swap; it is
-- written only by create/join/start/leave and the daily resolver. Hot per-player
-- action queues live in realm_actions (migration 193) so players never contend
-- on this row.
CREATE TABLE realm_games (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    status TEXT NOT NULL DEFAULT 'open'
        CHECK (status IN ('open', 'active', 'finished', 'cancelled')),
    creator_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    ruleset_id TEXT NOT NULL DEFAULT 'standard',
    chat_room_id UUID REFERENCES chat_rooms(id) ON DELETE SET NULL,
    -- UTC days since the Unix epoch of the last resolved day; the resolver
    -- catches up one day at a time while this trails the current UTC day.
    last_resolved_day INTEGER NOT NULL DEFAULT 0,
    winner_user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    state JSONB NOT NULL DEFAULT '{}'::jsonb
);

CREATE INDEX idx_realm_games_status ON realm_games(status);
