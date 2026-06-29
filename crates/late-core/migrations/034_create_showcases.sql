CREATE TABLE showcases (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    title TEXT NOT NULL CHECK (length(title) BETWEEN 1 AND 120),
    url TEXT NOT NULL CHECK (length(url) BETWEEN 1 AND 2000),
    description TEXT NOT NULL CHECK (length(description) BETWEEN 1 AND 800),
    tags TEXT[] NOT NULL DEFAULT ARRAY[]::TEXT[]
);

CREATE INDEX idx_showcases_created ON showcases (created DESC, id DESC);
CREATE INDEX idx_showcases_user ON showcases (user_id);
