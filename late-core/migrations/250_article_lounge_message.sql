-- A News share's card in #lounge: the message the share posted as the
-- sharer, which chat draws as the story's card (`app/chat/news/`). NULL for
-- a share from before cards, a share whose post the room refused, and once
-- the message is deleted: the story stays in News either way.
ALTER TABLE articles
    ADD COLUMN lounge_message_id UUID REFERENCES chat_messages(id) ON DELETE SET NULL;

-- One card per message. The paper's #lounge transcript leaves cards out by
-- this column, and the snapshot reads the newest cards through it.
CREATE UNIQUE INDEX articles_lounge_message_id_key
    ON articles (lounge_message_id)
    WHERE lounge_message_id IS NOT NULL;
