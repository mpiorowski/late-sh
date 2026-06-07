-- Whether a room is voice-enabled (VC) or text-only. Defaults to false so every
-- existing room stays text-only until a moderator turns voice on for it.
ALTER TABLE chat_rooms
    ADD COLUMN voice_enabled BOOLEAN NOT NULL DEFAULT false;
