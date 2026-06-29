CREATE TABLE showcase_feed_reads (
    user_id UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    last_read_at TIMESTAMPTZ,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp
);
