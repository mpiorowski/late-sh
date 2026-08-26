-- The crown: one slot, one holder, one glyph after their name in chat.
--
-- Taking it costs `max(5000, ceil(paid_chips * 1.5))` and burns every chip:
-- there is a `chip_crown_taken` debit and no matching credit anywhere, so
-- the price ratchets with whoever is willing to pay and nobody tunes it.
--
-- One row per reign. The live reign is the one with `ended_at IS NULL`, and
-- the partial unique index below makes "at most one" a table fact rather
-- than a service promise: a take closes the old row and inserts the new one
-- in the same transaction, so two concurrent takes cannot both land.
--
-- `month` is the first of the UTC month the reign was taken in. A reign is
-- current only while its month is, which is how the crown empties at the
-- rollover with no sweeper: the glyph resolver and `/crown` both compare
-- `month` against today's, and the next take closes whatever stale row is
-- still open. The month's last reign is also what the monthly profile-award
-- snapshot reads to grant the `crown` badge.
--
-- `paid_chips` is what the holder actually paid, not what the next taker
-- owes: the ladder is derived from it at read time, so a later change to
-- the multiplier cannot rewrite history.
CREATE TABLE crown_reigns (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    month DATE NOT NULL,
    holder_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    paid_chips BIGINT NOT NULL CHECK (paid_chips > 0),
    taken_at TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    ended_at TIMESTAMPTZ,
    CONSTRAINT crown_reigns_month_is_first CHECK (EXTRACT(DAY FROM month) = 1),
    CONSTRAINT crown_reigns_ends_after_it_starts CHECK (ended_at IS NULL OR ended_at >= taken_at)
);

-- At most one open reign, ever. Indexing the constant `ended_at IS NULL`
-- (always `true` for the rows the partial index covers) is what turns
-- "unique" into "at most one row total" rather than one per some column.
CREATE UNIQUE INDEX crown_reigns_single_open
    ON crown_reigns ((ended_at IS NULL))
    WHERE ended_at IS NULL;

-- The award snapshot's read: the last reign taken in a given month.
CREATE INDEX crown_reigns_month_taken_idx
    ON crown_reigns (month, taken_at DESC);
