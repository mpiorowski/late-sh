-- One row per directed request. accepted_at NULL = pending; non-NULL = mutual
-- friendship. Direction is preserved for display (who asked whom) but lookups
-- are symmetric — see Friendship::status() in late-core.
CREATE TABLE friendships (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    requester_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    addressee_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    accepted_at TIMESTAMPTZ,
    CONSTRAINT friendships_no_self CHECK (requester_id <> addressee_id),
    CONSTRAINT friendships_unique_directed UNIQUE (requester_id, addressee_id)
);

-- Anti-duplication across direction: if A→B exists, prevent B→A from being
-- inserted. The model layer auto-accepts a reverse pending instead.
CREATE UNIQUE INDEX friendships_unique_undirected
    ON friendships (LEAST(requester_id, addressee_id), GREATEST(requester_id, addressee_id));

CREATE INDEX idx_friendships_addressee_pending
    ON friendships (addressee_id) WHERE accepted_at IS NULL;
CREATE INDEX idx_friendships_requester_pending
    ON friendships (requester_id) WHERE accepted_at IS NULL;
CREATE INDEX idx_friendships_accepted_either
    ON friendships (requester_id, addressee_id) WHERE accepted_at IS NOT NULL;
