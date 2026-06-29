ALTER TABLE chat_message_reactions
DROP CONSTRAINT IF EXISTS chat_message_reactions_kind_check;

ALTER TABLE chat_message_reactions
ADD CONSTRAINT chat_message_reactions_kind_check
CHECK (kind BETWEEN 1 AND 8);
