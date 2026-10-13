-- Every board post ever made, one row each, written in the post's own
-- transaction. The daily cap counts these rows, so deleting a post does not
-- give its slot back. Rows are never updated and leave only with the account.
CREATE TABLE calendar_posts (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE
);
CREATE INDEX calendar_posts_per_day ON calendar_posts(user_id, created);

-- The board posts already up count as made.
INSERT INTO calendar_posts (created, user_id)
SELECT e.created, e.creator_id
FROM calendar_events e
JOIN users u ON u.id = e.creator_id
WHERE e.owner_id IS NULL;

-- The cap no longer reads calendar_events.
DROP INDEX calendar_event_posts_per_day;
