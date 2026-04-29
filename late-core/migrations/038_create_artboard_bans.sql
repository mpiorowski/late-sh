CREATE TABLE artboard_bans (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    target_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    actor_user_id UUID NOT NULL REFERENCES users(id),
    reason TEXT NOT NULL DEFAULT '',
    expires_at TIMESTAMPTZ,
    UNIQUE (target_user_id)
);

CREATE INDEX idx_artboard_bans_expires_at
    ON artboard_bans (expires_at);
