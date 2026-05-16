CREATE TABLE media_queue_items (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    submitter_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    media_kind TEXT NOT NULL DEFAULT 'youtube'
        CHECK (media_kind IN ('youtube')),
    external_id TEXT NOT NULL CHECK (length(trim(external_id)) > 0),
    title TEXT,
    channel TEXT,
    duration_ms INTEGER CHECK (duration_ms IS NULL OR duration_ms >= 0),
    is_stream BOOLEAN NOT NULL DEFAULT false,
    status TEXT NOT NULL DEFAULT 'queued'
        CHECK (status IN ('queued', 'playing', 'played', 'skipped', 'failed')),
    started_at TIMESTAMPTZ,
    ended_at TIMESTAMPTZ,
    error TEXT
);

CREATE INDEX idx_media_queue_status_created
    ON media_queue_items(status, created);

CREATE INDEX idx_media_queue_submitter_created
    ON media_queue_items(submitter_id, created DESC);

CREATE UNIQUE INDEX idx_media_queue_single_playing
    ON media_queue_items ((true))
    WHERE status = 'playing';
