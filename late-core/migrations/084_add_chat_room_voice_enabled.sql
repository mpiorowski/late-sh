-- Whether a room is voice-enabled (VC) or text-only. Chat rooms default to
-- text-only; game rooms get a voice channel by default since voice chat is the
-- point of playing together. A moderator can flip either with room-voice.
ALTER TABLE chat_rooms
    ADD COLUMN voice_enabled BOOLEAN NOT NULL DEFAULT false;

UPDATE chat_rooms SET voice_enabled = true WHERE kind = 'game';
