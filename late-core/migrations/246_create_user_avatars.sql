CREATE TABLE user_avatars (
    user_id UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    size SMALLINT NOT NULL CHECK (size BETWEEN 4 AND 32),
    canvas JSONB NOT NULL,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp
);
