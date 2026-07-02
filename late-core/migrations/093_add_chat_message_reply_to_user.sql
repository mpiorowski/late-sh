ALTER TABLE chat_messages
ADD COLUMN reply_to_user_id UUID REFERENCES users(id) ON DELETE SET NULL;

CREATE INDEX idx_chat_messages_reply_to_user
ON chat_messages (reply_to_user_id)
WHERE reply_to_user_id IS NOT NULL;
