CREATE TABLE profiles (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    user_id UUID NOT NULL UNIQUE REFERENCES users(id) ON DELETE CASCADE,
    username TEXT NOT NULL DEFAULT '' CHECK (length(btrim(username)) BETWEEN 1 AND 32),
    enable_ghost BOOLEAN NOT NULL DEFAULT true
);

CREATE UNIQUE INDEX idx_profiles_user ON profiles (user_id);
CREATE UNIQUE INDEX idx_profiles_username_lower ON profiles (LOWER(username));
