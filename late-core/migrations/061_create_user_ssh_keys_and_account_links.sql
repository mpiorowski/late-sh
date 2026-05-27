CREATE TABLE user_ssh_keys (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    last_seen TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    fingerprint TEXT NOT NULL UNIQUE,
    label TEXT
);

CREATE INDEX idx_user_ssh_keys_user_id ON user_ssh_keys(user_id);
CREATE INDEX idx_user_ssh_keys_fingerprint ON user_ssh_keys(fingerprint);

INSERT INTO user_ssh_keys (user_id, fingerprint, created, updated, last_seen)
SELECT id, fingerprint, created, updated, last_seen
FROM users
ON CONFLICT (fingerprint) DO NOTHING;

CREATE TABLE account_link_codes (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    code TEXT NOT NULL UNIQUE,
    expires_at TIMESTAMPTZ NOT NULL,
    consumed_at TIMESTAMPTZ
);

CREATE INDEX idx_account_link_codes_user_id ON account_link_codes(user_id);
CREATE INDEX idx_account_link_codes_code ON account_link_codes(code);
