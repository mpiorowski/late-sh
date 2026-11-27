-- The gallery's closing week: a few #lounge lines, on fixed days before the
-- month's wall closes, that ask the room to applaud. One row per call. The
-- unique (month, days left) is the claim: every replica tries the insert at
-- the posting hour and only the one that wins posts the line.
--
-- `message_id` is the line, which Enter opens the Vote listing from. A
-- deleted line (a moderator's) leaves the row, so the call stays claimed
-- and is not posted again.
CREATE TABLE artboard_vote_calls (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    period_month DATE NOT NULL,
    days_left INT NOT NULL CHECK (days_left > 0),
    message_id UUID REFERENCES chat_messages(id) ON DELETE SET NULL,
    UNIQUE (period_month, days_left)
);

CREATE UNIQUE INDEX artboard_vote_calls_message_id_key
    ON artboard_vote_calls (message_id)
    WHERE message_id IS NOT NULL;

-- Every replica keeps the calls' message ids next to the piece cards and
-- re-reads both on the same signal: a line linked, or deleted.
CREATE TRIGGER artboard_vote_calls_changed
    AFTER UPDATE OF message_id ON artboard_vote_calls
    FOR EACH ROW
    WHEN (OLD.message_id IS DISTINCT FROM NEW.message_id)
    EXECUTE FUNCTION notify_artboard_pieces_changed();
