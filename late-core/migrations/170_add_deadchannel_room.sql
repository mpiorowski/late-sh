-- Add the 'deadchannel' kind: the game's haunted channel (GAME.md, First
-- contact stage 4). A dedicated kind so every room listing excludes it by
-- construction (browse lists only 'topic', IRC lists lounge/language/topic)
-- and joining can be gated on the first-contact invitation instead of the
-- public join path.

ALTER TABLE chat_rooms DROP CONSTRAINT chat_rooms_kind_check;
ALTER TABLE chat_rooms ADD CONSTRAINT chat_rooms_kind_check
    CHECK (kind IN ('lounge', 'language', 'dm', 'topic', 'game', 'deadchannel'));

-- Never auto-join, always addressable: the channel opens only through a
-- consented /join from an invited user.
ALTER TABLE chat_rooms ADD CONSTRAINT chat_rooms_deadchannel_chk
    CHECK (kind <> 'deadchannel' OR (auto_join = false AND slug IS NOT NULL));

CREATE UNIQUE INDEX uq_chat_rooms_deadchannel_slug
ON chat_rooms (slug)
WHERE kind = 'deadchannel';
