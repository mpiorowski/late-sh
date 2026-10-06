-- Drop admin message pins: the Home pinned strip is retired in favour of
-- room topics (see 125_add_chat_room_topic.sql), which is where high-traffic
-- announcements now live.
--
-- This sat in migrations/deferred/ for a release: the pre-#469 binary read
-- `pinned` out of every SELECT * row, and drained pods live for hours after
-- a deploy, so dropping the column in the same release would have panicked
-- every chat load in the old pod. #469 shipped the pins-removal code, so
-- those pods are long gone and the column is now unreferenced.

DROP INDEX IF EXISTS idx_chat_messages_pinned_created;

ALTER TABLE chat_messages
DROP COLUMN pinned;
