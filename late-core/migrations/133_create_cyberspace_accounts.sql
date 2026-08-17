-- Linked cyberspace.online accounts. late.sh acts as a personal client for
-- the linked user: we store only the Firebase refresh token (never the
-- password), and every API call happens as that user under their own token.
CREATE TABLE cyberspace_accounts (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    user_id UUID NOT NULL UNIQUE REFERENCES users(id) ON DELETE CASCADE,
    cs_user_id TEXT NOT NULL CHECK (length(cs_user_id) BETWEEN 1 AND 128),
    cs_username TEXT NOT NULL CHECK (length(cs_username) BETWEEN 1 AND 64),
    refresh_token TEXT NOT NULL CHECK (length(refresh_token) BETWEEN 1 AND 4096)
);
