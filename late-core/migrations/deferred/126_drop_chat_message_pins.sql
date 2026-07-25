-- DEFERRED: promote to migrations/ (with a fresh number) in the release
-- AFTER the pins-removal code ships. The previous binary reads `pinned` out
-- of every SELECT * row, and drained pods live for hours after a deploy, so
-- dropping the column in the same release panics every chat load in the old
-- pod. Files in this folder are invisible to the migration runner; build.rs
-- warns on every build while any remain here.
--
-- Drop admin message pins: the Home pinned strip is retired in favour of
-- room topics (see 125_add_chat_room_topic.sql), which is where high-traffic
-- announcements now live.

DROP INDEX IF EXISTS idx_chat_messages_pinned_created;

ALTER TABLE chat_messages
DROP COLUMN pinned;
