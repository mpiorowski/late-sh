CREATE TABLE native_tokens (
    token      TEXT        NOT NULL PRIMARY KEY,
    user_id    UUID        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX native_tokens_user_id_idx  ON native_tokens (user_id);
CREATE INDEX native_tokens_expires_at_idx ON native_tokens (expires_at);
