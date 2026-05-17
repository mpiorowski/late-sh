CREATE TABLE media_queue_votes (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    item_id UUID NOT NULL REFERENCES media_queue_items(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    value SMALLINT NOT NULL CHECK (value IN (-1, 1))
);

CREATE UNIQUE INDEX idx_media_queue_votes_user_item
    ON media_queue_votes (user_id, item_id);

CREATE INDEX idx_media_queue_votes_item
    ON media_queue_votes (item_id);
