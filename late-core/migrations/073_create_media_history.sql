CREATE TABLE media_history_items (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    media_kind TEXT NOT NULL DEFAULT 'youtube'
        CHECK (media_kind IN ('youtube')),
    external_id TEXT NOT NULL CHECK (length(trim(external_id)) > 0),
    title TEXT,
    channel TEXT,
    duration_ms INTEGER CHECK (duration_ms IS NULL OR duration_ms >= 0),
    is_stream BOOLEAN NOT NULL DEFAULT false,
    first_played_at TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    last_played_at TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    play_count INTEGER NOT NULL DEFAULT 1 CHECK (play_count >= 1),
    last_submitter_id UUID REFERENCES users(id) ON DELETE SET NULL
);

CREATE UNIQUE INDEX idx_media_history_kind_external
    ON media_history_items(media_kind, external_id);

CREATE INDEX idx_media_history_last_played
    ON media_history_items(last_played_at DESC);

CREATE TABLE media_history_votes (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    item_id UUID NOT NULL REFERENCES media_history_items(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    value SMALLINT NOT NULL CHECK (value IN (-1, 1))
);

CREATE UNIQUE INDEX idx_media_history_votes_user_item
    ON media_history_votes (user_id, item_id);

CREATE INDEX idx_media_history_votes_item
    ON media_history_votes (item_id);
