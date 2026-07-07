ALTER TABLE chat_message_reactions
ADD COLUMN icon TEXT;

UPDATE chat_message_reactions
SET icon = CASE kind
    WHEN 0 THEN '👋'
    WHEN 1 THEN '👍'
    WHEN 2 THEN '🧡'
    WHEN 3 THEN '😂'
    WHEN 4 THEN '👀'
    WHEN 5 THEN '🔥'
    WHEN 6 THEN '🙌'
    WHEN 7 THEN '🚀'
    WHEN 8 THEN '🤔'
    WHEN 9 THEN '💩'
    ELSE '?'
END;

ALTER TABLE chat_message_reactions
ALTER COLUMN icon SET NOT NULL;

ALTER TABLE chat_message_reactions
ADD CONSTRAINT chat_message_reactions_icon_check
CHECK (length(btrim(icon)) > 0 AND char_length(icon) <= 64);

DROP INDEX IF EXISTS idx_chat_message_reactions_message_kind;

ALTER TABLE chat_message_reactions
DROP CONSTRAINT IF EXISTS chat_message_reactions_kind_check;

ALTER TABLE chat_message_reactions
DROP COLUMN kind;

CREATE INDEX idx_chat_message_reactions_message_icon
ON chat_message_reactions (message_id, icon);
