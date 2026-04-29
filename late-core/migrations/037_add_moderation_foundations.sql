ALTER TABLE users
ADD COLUMN is_moderator BOOLEAN NOT NULL DEFAULT FALSE;

CREATE TABLE moderation_audit_log (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    actor_user_id UUID NOT NULL REFERENCES users(id),
    action TEXT NOT NULL,
    target_kind TEXT NOT NULL,
    target_id UUID,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb
);

CREATE INDEX idx_moderation_audit_log_actor_user_id
    ON moderation_audit_log (actor_user_id);

CREATE INDEX idx_moderation_audit_log_target_kind_target_id
    ON moderation_audit_log (target_kind, target_id);

CREATE TABLE room_bans (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    room_id UUID NOT NULL REFERENCES chat_rooms(id) ON DELETE CASCADE,
    target_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    actor_user_id UUID NOT NULL REFERENCES users(id),
    reason TEXT NOT NULL DEFAULT '',
    expires_at TIMESTAMPTZ,
    UNIQUE (room_id, target_user_id)
);

CREATE INDEX idx_room_bans_target_user_id
    ON room_bans (target_user_id);

CREATE INDEX idx_room_bans_expires_at
    ON room_bans (expires_at);

CREATE TABLE server_bans (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    ban_type TEXT NOT NULL,
    target_user_id UUID REFERENCES users(id) ON DELETE CASCADE,
    fingerprint TEXT,
    ip_address TEXT,
    snapshot_username TEXT,
    actor_user_id UUID NOT NULL REFERENCES users(id),
    reason TEXT NOT NULL DEFAULT '',
    expires_at TIMESTAMPTZ,
    CHECK (ban_type IN ('user', 'fingerprint', 'ip')),
    CHECK (
        (ban_type = 'user' AND target_user_id IS NOT NULL)
        OR (ban_type = 'fingerprint' AND fingerprint IS NOT NULL)
        OR (ban_type = 'ip' AND ip_address IS NOT NULL)
    )
);

CREATE INDEX idx_server_bans_target_user_id
    ON server_bans (target_user_id);

CREATE INDEX idx_server_bans_ban_type
    ON server_bans (ban_type);

CREATE INDEX idx_server_bans_fingerprint
    ON server_bans (fingerprint);

CREATE INDEX idx_server_bans_ip_address
    ON server_bans (ip_address);

CREATE INDEX idx_server_bans_expires_at
    ON server_bans (expires_at);
