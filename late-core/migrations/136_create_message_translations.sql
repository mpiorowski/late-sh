-- Cached chat-message translations, one row per (message, target language).
-- The cache is what makes translation cost scale with messages written
-- instead of readers: the first viewer's Gemini call lands here and every
-- later viewer (and reconnecting session) reads the row for free.
--
-- Rows die with their message via the FK cascade; edits invalidate
-- explicitly inside the edit transaction (chat svc), since a translated
-- body must never outlive the text it translated.

CREATE TABLE message_translations (
    message_id UUID NOT NULL REFERENCES chat_messages(id) ON DELETE CASCADE,
    target_lang VARCHAR NOT NULL,
    body TEXT NOT NULL,
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    PRIMARY KEY (message_id, target_lang)
);
