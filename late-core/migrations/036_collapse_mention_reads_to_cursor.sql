CREATE TABLE mention_feed_reads (
    user_id UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    last_read_at TIMESTAMPTZ,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp
);

INSERT INTO mention_feed_reads (user_id, last_read_at, updated)
SELECT user_id, MAX(read_at), current_timestamp
FROM notifications
WHERE read_at IS NOT NULL
GROUP BY user_id;

DROP INDEX IF EXISTS idx_notifications_user_unread;

ALTER TABLE notifications
DROP COLUMN read_at;
