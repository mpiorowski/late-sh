CREATE TABLE work_profiles (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    slug TEXT NOT NULL UNIQUE CHECK (slug ~ '^w_[a-z0-9]{12}$'),
    headline TEXT NOT NULL CHECK (length(headline) BETWEEN 1 AND 120),
    status TEXT NOT NULL CHECK (status IN ('open', 'casual', 'not-looking')),
    work_type TEXT NOT NULL CHECK (length(work_type) BETWEEN 1 AND 80),
    location TEXT NOT NULL CHECK (length(location) BETWEEN 1 AND 120),
    links TEXT[] NOT NULL DEFAULT ARRAY[]::TEXT[] CHECK (cardinality(links) BETWEEN 1 AND 6),
    skills TEXT[] NOT NULL DEFAULT ARRAY[]::TEXT[] CHECK (cardinality(skills) <= 12),
    summary TEXT NOT NULL CHECK (length(summary) BETWEEN 1 AND 1000),
    UNIQUE (user_id)
);

CREATE INDEX idx_work_profiles_updated ON work_profiles (updated DESC, created DESC, id DESC);
CREATE INDEX idx_work_profiles_user ON work_profiles (user_id);
CREATE INDEX idx_work_profiles_status ON work_profiles (status);

CREATE TABLE work_feed_reads (
    user_id UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    last_read_at TIMESTAMPTZ,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp
);
