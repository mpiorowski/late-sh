-- Lines carved into the bar out back (late-ssh/src/app/clubhouse/nightcap):
-- one per stool, written by whoever sits there, kept until the next sitter
-- carves over it. The carver is kept for accountability and cleared, not
-- cascaded, when the account goes: the wood remembers the words.

CREATE TABLE nightcap_carvings (
    stool SMALLINT PRIMARY KEY CHECK (stool >= 0 AND stool < 6),
    user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    body TEXT NOT NULL CHECK (char_length(body) BETWEEN 1 AND 60),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp
);
