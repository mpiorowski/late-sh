-- Voice is its own channel domain. Chat rooms, game rooms, and future
-- activities can opt into voice without pretending voice is chat metadata.
CREATE TABLE voice_channels (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    target_kind TEXT NOT NULL CHECK (target_kind IN ('chat_room', 'game_room')),
    target_id UUID NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT true,
    display_name TEXT NOT NULL,

    CONSTRAINT voice_channels_display_name_chk CHECK (length(trim(display_name)) > 0),
    CONSTRAINT voice_channels_target_unique UNIQUE (target_kind, target_id)
);

CREATE INDEX idx_voice_channels_enabled_target
ON voice_channels (enabled, target_kind, target_id);

-- Existing game rooms get voice by default. Chat rooms stay text-only until
-- a moderator creates/enables a chat-room voice channel, except DMs/private
-- rooms which are voice-first by default.
INSERT INTO voice_channels (target_kind, target_id, enabled, display_name)
SELECT 'game_room', id, true, display_name
FROM game_rooms
WHERE status <> 'closed'
ON CONFLICT (target_kind, target_id) DO NOTHING;

INSERT INTO voice_channels (target_kind, target_id, enabled, display_name)
SELECT 'chat_room', id, true, COALESCE(slug, kind)
FROM chat_rooms
WHERE visibility IN ('dm', 'private')
ON CONFLICT (target_kind, target_id) DO NOTHING;
