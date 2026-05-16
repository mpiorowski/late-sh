CREATE TABLE media_sources (
    id              UUID PRIMARY KEY DEFAULT uuidv7(),
    created         TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated         TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    source_kind     TEXT NOT NULL CHECK (source_kind IN ('youtube_fallback')),
    media_kind      TEXT NOT NULL DEFAULT 'youtube'
                    CHECK (media_kind IN ('youtube')),
    external_id     TEXT NOT NULL CHECK (length(trim(external_id)) > 0),
    title           TEXT,
    channel         TEXT,
    is_stream       BOOLEAN NOT NULL DEFAULT true,
    updated_by      UUID REFERENCES users(id) ON DELETE SET NULL
);

CREATE UNIQUE INDEX idx_media_sources_source_kind
    ON media_sources (source_kind);
