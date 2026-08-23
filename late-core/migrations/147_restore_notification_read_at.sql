-- Restore the per-mention read stamp that 036 collapsed into the global
-- mention_feed_reads cursor. The global watermark still clears everything
-- when the Mentions entry is opened, but the rail badge also clears when a
-- mention's message is actually rendered in its own room, and only a per-row
-- stamp can express "this mention was seen" without also swallowing mentions
-- above the loaded tail the way a room-level cursor would.
ALTER TABLE notifications
ADD COLUMN read_at TIMESTAMPTZ;

CREATE INDEX idx_notifications_user_unread
ON notifications (user_id, created DESC)
WHERE read_at IS NULL;
