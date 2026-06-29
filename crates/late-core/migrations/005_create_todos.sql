CREATE TABLE todos (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    body TEXT NOT NULL CHECK (length(trim(body)) > 0 AND length(body) <= 280),
    done BOOLEAN NOT NULL DEFAULT false,
    completed_at TIMESTAMPTZ
);

CREATE INDEX idx_todos_user_updated
ON todos (user_id, updated DESC, id DESC);

CREATE INDEX idx_todos_user_done_updated
ON todos (user_id, done, updated DESC, id DESC);
