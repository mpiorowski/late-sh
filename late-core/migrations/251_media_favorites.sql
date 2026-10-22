-- One row per Booth track a listener keeps (`f` in the booth's History
-- list). Favorites sit on top of that listener's History list, at most
-- BOOTH_FAVORITES_LIMIT each (late_core::models::media_favorite), the cap
-- enforced in the add transaction, never here. The row carries its own copy
-- of the validated track metadata, so the History prune and a staff delete
-- of the history row never take a favorite with them.
CREATE TABLE media_favorites (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    media_kind TEXT NOT NULL DEFAULT 'youtube'
        CHECK (media_kind IN ('youtube')),
    external_id TEXT NOT NULL CHECK (length(trim(external_id)) > 0),
    title TEXT,
    channel TEXT,
    duration_ms INTEGER CHECK (duration_ms IS NULL OR duration_ms >= 0),
    is_stream BOOLEAN NOT NULL DEFAULT false
);

-- A track is one favorite per listener; the listing walks the same index.
CREATE UNIQUE INDEX media_favorites_user_track_key
    ON media_favorites (user_id, media_kind, external_id);
