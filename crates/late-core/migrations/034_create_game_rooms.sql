-- Generic registry for game-backed rooms. Chat membership, privacy, unread
-- state, and messages stay in chat_rooms/chat_room_members/chat_messages.
-- Game-specific runtime/persistence hangs off this row by game_room_id.

CREATE TABLE game_rooms (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,

    chat_room_id UUID NOT NULL UNIQUE REFERENCES chat_rooms(id) ON DELETE CASCADE,
    game_kind TEXT NOT NULL,
    slug TEXT NOT NULL UNIQUE,
    display_name TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'open',
    settings JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_by UUID REFERENCES users(id) ON DELETE SET NULL,

    CONSTRAINT game_rooms_game_kind_chk CHECK (game_kind <> ''),
    CONSTRAINT game_rooms_slug_chk CHECK (slug <> ''),
    CONSTRAINT game_rooms_display_name_chk CHECK (display_name <> ''),
    CONSTRAINT game_rooms_status_chk CHECK (status IN ('open', 'in_round', 'paused', 'closed'))
);

CREATE INDEX idx_game_rooms_game_kind_status
ON game_rooms (game_kind, status, created);
