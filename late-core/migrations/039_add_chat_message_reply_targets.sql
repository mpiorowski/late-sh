ALTER TABLE chat_messages
ADD COLUMN reply_to_message_id UUID REFERENCES chat_messages(id) ON DELETE SET NULL;

CREATE INDEX idx_chat_messages_reply_to
ON chat_messages (reply_to_message_id)
WHERE reply_to_message_id IS NOT NULL;
