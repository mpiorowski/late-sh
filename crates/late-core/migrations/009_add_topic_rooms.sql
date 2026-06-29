-- Add 'topic' kind to chat_rooms for user-created public rooms.

-- Drop and recreate the kind CHECK constraint to include 'topic'.
ALTER TABLE chat_rooms DROP CONSTRAINT chat_rooms_kind_check;
ALTER TABLE chat_rooms ADD CONSTRAINT chat_rooms_kind_check
    CHECK (kind IN ('general', 'language', 'dm', 'topic'));

-- Unique slug per topic room.
CREATE UNIQUE INDEX uq_chat_rooms_topic_slug
ON chat_rooms (slug)
WHERE kind = 'topic';
