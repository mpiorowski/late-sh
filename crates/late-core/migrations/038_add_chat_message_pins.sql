-- Allow admins to pin chat messages for dashboard visibility.

ALTER TABLE chat_messages
ADD COLUMN pinned BOOLEAN NOT NULL DEFAULT false;

CREATE INDEX idx_chat_messages_pinned_created
ON chat_messages (created DESC, id DESC)
WHERE pinned = true;
