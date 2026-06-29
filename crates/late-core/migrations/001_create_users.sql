-- Users table: SSH fingerprint is the identity
CREATE TABLE users (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    last_seen TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    fingerprint TEXT NOT NULL,
    username TEXT NOT NULL DEFAULT '',
    settings JSONB NOT NULL DEFAULT '{}',
    UNIQUE (fingerprint)
);

CREATE INDEX idx_users_fingerprint ON users (fingerprint);
CREATE INDEX idx_users_last_seen ON users (last_seen);
