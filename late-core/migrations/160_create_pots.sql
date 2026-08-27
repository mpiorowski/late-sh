-- The pot: a daily parimutuel raffle, and the biggest chip sink that works
-- at any concurrency (SHOP.md phase 5).
--
-- Tickets cost a flat price, capped per user per pot. At the draw one ticket
-- is pulled, weighted by how many each player holds, and 80% of what the
-- tickets paid goes to whoever holds it. The other 20% has no credit row
-- anywhere: that gap is the burn, exactly like the gild's missing third.
--
-- There is no house wallet and no stored running total. A live pot's size is
-- `SUM(pot_tickets.count) * ticket_price`, so the tickets are the only
-- witness of what is at stake; `ticket_count` and `payout_chips` below are
-- stamped once, at the draw, as the settled record of what happened.
--
-- Exactly one pot is open at a time (the partial unique index below), and the
-- draw opens the next one in the same transaction, so `/pot` never has to
-- answer "there isn't one".
CREATE TABLE pots (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    opens_at TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    draws_at TIMESTAMPTZ NOT NULL,
    status TEXT NOT NULL DEFAULT 'open',
    ticket_price BIGINT NOT NULL CHECK (ticket_price > 0),
    -- Stamped at the draw and never again. NULL while the pot is open, on a
    -- pot that rolled with nobody in it, and on a settled pot whose winner
    -- later deleted their account: history keeps what was paid, and the
    -- ledger keeps who was paid.
    winner_user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    ticket_count BIGINT,
    payout_chips BIGINT,
    drawn_at TIMESTAMPTZ,
    -- The largest size threshold whose #lounge line has already been
    -- claimed, so "once each per pot" survives a restart and two replicas:
    -- the claim is an UPDATE guarded on this column, not a flag in memory.
    announced_threshold BIGINT NOT NULL DEFAULT 0 CHECK (announced_threshold >= 0),
    CONSTRAINT pots_status_is_known CHECK (status IN ('open', 'drawn', 'rolled')),
    -- One shape per status, so a half-settled pot cannot be written at all.
    CONSTRAINT pots_open_is_unsettled CHECK (
        status <> 'open'
        OR (drawn_at IS NULL AND winner_user_id IS NULL AND ticket_count IS NULL AND payout_chips IS NULL)
    ),
    -- A drawn pot had tickets in it and paid one of them; `ticket_count > 0`
    -- is what tells it apart from a rolled one, rather than the winner, whose
    -- account can be deleted out from under a settled row.
    CONSTRAINT pots_drawn_paid_out CHECK (
        status <> 'drawn'
        OR (drawn_at IS NOT NULL AND ticket_count > 0 AND payout_chips > 0)
    ),
    CONSTRAINT pots_rolled_is_empty CHECK (
        status <> 'rolled'
        OR (drawn_at IS NOT NULL AND winner_user_id IS NULL AND ticket_count = 0 AND payout_chips = 0)
    )
);

-- At most one open pot, ever. Indexing the constant `status = 'open'`
-- (always `true` for the rows the partial index covers) is what turns
-- "unique" into "at most one row total", the same trick migration 156 uses
-- for the crown's single open reign.
CREATE UNIQUE INDEX pots_single_open
    ON pots ((status = 'open'))
    WHERE status = 'open';

-- One row per buy, never updated: a player's holding in a pot is the sum of
-- their rows, and the ledger has one debit per row.
CREATE TABLE pot_tickets (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    pot_id UUID NOT NULL REFERENCES pots(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    count BIGINT NOT NULL CHECK (count > 0),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp
);

-- The two reads: the whole pot's holders (the draw and the snapshot) and one
-- player's holding (the per-user cap).
CREATE INDEX pot_tickets_pot_user_idx ON pot_tickets (pot_id, user_id);
