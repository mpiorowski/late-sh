CREATE TABLE rss_feeds (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    url TEXT NOT NULL CHECK (length(url) BETWEEN 1 AND 2000),
    title TEXT NOT NULL DEFAULT '',
    active BOOLEAN NOT NULL DEFAULT true,
    last_checked_at TIMESTAMPTZ,
    last_success_at TIMESTAMPTZ,
    last_error TEXT,
    UNIQUE (user_id, url)
);

CREATE INDEX idx_rss_feeds_user_created ON rss_feeds (user_id, created DESC);
CREATE INDEX idx_rss_feeds_active_checked ON rss_feeds (active, last_checked_at);

CREATE TABLE rss_entries (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    feed_id UUID NOT NULL REFERENCES rss_feeds(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    guid TEXT NOT NULL CHECK (length(guid) BETWEEN 1 AND 2000),
    url TEXT NOT NULL CHECK (length(url) BETWEEN 1 AND 2000),
    title TEXT NOT NULL CHECK (length(title) BETWEEN 1 AND 500),
    summary TEXT NOT NULL DEFAULT '',
    published_at TIMESTAMPTZ,
    shared_at TIMESTAMPTZ,
    dismissed_at TIMESTAMPTZ,
    UNIQUE (feed_id, guid),
    UNIQUE (user_id, url)
);

CREATE INDEX idx_rss_entries_user_created ON rss_entries (user_id, created DESC);
CREATE INDEX idx_rss_entries_user_unshared ON rss_entries (user_id, created DESC)
    WHERE shared_at IS NULL AND dismissed_at IS NULL;
