CREATE TABLE article_feed_reads (
    user_id UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    last_read_created TIMESTAMPTZ,
    last_read_article_id UUID REFERENCES articles(id) ON DELETE SET NULL,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    CONSTRAINT article_feed_reads_checkpoint_chk
        CHECK (
            (last_read_created IS NULL AND last_read_article_id IS NULL)
            OR (last_read_created IS NOT NULL AND last_read_article_id IS NOT NULL)
        )
);
