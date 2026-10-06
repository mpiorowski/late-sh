-- Message search: substring (ILIKE) search over chat_messages.body.
-- pg_trgm makes ILIKE '%query%' indexable; the index floor is 3 chars,
-- matching the TUI's minimum search query length.
CREATE EXTENSION IF NOT EXISTS pg_trgm;

CREATE INDEX IF NOT EXISTS idx_chat_messages_body_trgm
    ON chat_messages USING gin (body gin_trgm_ops);
