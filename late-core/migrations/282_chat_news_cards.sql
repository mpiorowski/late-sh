-- A News card must be excluded from chat statistics before its article link
-- lands, and even if linking fails or the article is subsequently removed.
ALTER TABLE chat_messages ADD COLUMN is_news_card BOOLEAN NOT NULL DEFAULT false;

UPDATE chat_messages m SET is_news_card = true
FROM articles a WHERE a.lounge_message_id = m.id;
