CREATE TABLE irc_tokens (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    user_id UUID NOT NULL UNIQUE REFERENCES users(id) ON DELETE CASCADE,
    token_hash TEXT NOT NULL UNIQUE,
    last_used TIMESTAMPTZ
);
