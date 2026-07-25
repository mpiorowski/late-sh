-- Drop admin message pins: the Home pinned strip is retired in favour of
-- room topics (see 125_add_chat_room_topic.sql), which is where high-traffic
-- announcements now live.

DROP INDEX IF EXISTS idx_chat_messages_pinned_created;

ALTER TABLE chat_messages
DROP COLUMN pinned;
