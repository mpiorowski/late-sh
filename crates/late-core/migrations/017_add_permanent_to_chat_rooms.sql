-- Add permanent flag to chat_rooms.
-- Permanent rooms: auto-join for all users, cannot be left.

ALTER TABLE chat_rooms ADD COLUMN permanent BOOLEAN NOT NULL DEFAULT false;

-- #general is the original permanent room.
UPDATE chat_rooms SET permanent = true WHERE kind = 'general' AND slug = 'general';
