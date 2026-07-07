CREATE TABLE chat_slow_modes (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    room_id UUID NOT NULL REFERENCES chat_rooms(id) ON DELETE CASCADE,
    target_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    actor_user_id UUID NOT NULL REFERENCES users(id),
    interval_secs INTEGER NOT NULL CHECK (interval_secs BETWEEN 1 AND 86400),
    reason TEXT NOT NULL DEFAULT '',
    expires_at TIMESTAMPTZ,
    UNIQUE (room_id, target_user_id)
);

CREATE INDEX idx_chat_slow_modes_target_user_id
    ON chat_slow_modes (target_user_id);

CREATE INDEX idx_chat_slow_modes_expires_at
    ON chat_slow_modes (expires_at);
