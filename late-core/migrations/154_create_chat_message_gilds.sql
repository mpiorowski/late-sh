-- Gilds: paying chips to mark someone else's message, permanently.
--
-- A gild is a purchase, not a toggle. There is no un-gild and no update path,
-- so the row carries `created` alone: nothing about it can ever change.
--
-- `author_user_id` is denormalized off `chat_messages` for two reasons. It
-- makes "no self-gild" a table constraint rather than a service promise (the
-- shape of `notifications_no_self_mention`, migration 020), and it lets the
-- profile count read one owner-scoped query instead of joining every gilded
-- message back to its author.
--
-- Chips do not live here. The 2/3 the author receives and the 1/3 that is
-- never re-minted are `chip_ledger` rows (`chip_gild_sent` / `chip_gild_received`);
-- `chips` records what the buyer paid at the tier's price on the day, so a
-- later reprice cannot rewrite history.
CREATE TABLE chat_message_gilds (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    message_id UUID NOT NULL REFERENCES chat_messages(id) ON DELETE CASCADE,
    author_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    tier SMALLINT NOT NULL CHECK (tier BETWEEN 1 AND 3),
    chips BIGINT NOT NULL CHECK (chips > 0),
    CONSTRAINT chat_message_gilds_no_self_gild CHECK (user_id <> author_user_id)
);

-- One gild per buyer per message per tier: a whale may stack all three, but
-- nobody buys the same tier twice on the same message.
CREATE UNIQUE INDEX chat_message_gilds_once_per_tier
    ON chat_message_gilds (message_id, user_id, tier);

-- The marker query: one pass per page of messages, keyed by message.
CREATE INDEX chat_message_gilds_message_idx
    ON chat_message_gilds (message_id);

-- The profile query: gilds received, scoped to the profile's owner.
CREATE INDEX chat_message_gilds_author_idx
    ON chat_message_gilds (author_user_id);
