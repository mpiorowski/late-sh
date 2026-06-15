CREATE INDEX chat_polls_active_ends_idx
    ON chat_polls (ends_at, id)
    WHERE active = true;
