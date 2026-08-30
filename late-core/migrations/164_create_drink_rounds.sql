-- The round: one patron buys a drink for everyone else at the bar
-- (SHOP.md, "The round").
--
-- The buyer pays `price_per_patron` for every credit the round actually
-- granted, burned whole: there is no credit row anywhere, exactly like the
-- crown. What the patrons get is not a drink, it is the right to one. Nobody
-- is poured into without asking for it, because a poured drink makes a
-- patron type drunk in public; the credit is consent deferred until they walk
-- up and order from @bartender themselves.
--
-- There is no stored total on the round. The `chip_ledger` row keyed on the
-- round id is the record of what was paid and, by the price, of how many it
-- bought. The credit rows are not that record: a later round takes over a
-- patron's expired credit in place (see the unique index below), so an old
-- round's roster shrinks after the fact. That is what lets the round row be
-- written before the grant is counted, with nothing to keep in step
-- afterwards.
CREATE TABLE drink_rounds (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    -- The round survives its buyer leaving; the credits it granted are still
    -- good, and the #lounge line already named them.
    buyer_user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    price_per_patron BIGINT NOT NULL CHECK (price_per_patron > 0),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp
);

-- One row per patron per round: an open tab at the bar, cashed by ordering.
CREATE TABLE drink_credits (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    round_id UUID NOT NULL REFERENCES drink_rounds(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    -- Uncashed credits die on their own; nothing sweeps them. A patron who
    -- was asleep when the round landed does not wake up owed a drink from
    -- last week, and the expiry is what a re-grant checks against.
    expires_at TIMESTAMPTZ NOT NULL,
    -- Stamped once, by the pour that spent it. NULL is an open credit.
    cashed_at TIMESTAMPTZ,
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp
);

-- At most one open credit per patron, ever, across every round. Two things
-- rest on this. A second round moments after the first grants almost nothing
-- and so costs almost nothing, which is the only throttle the mechanic needs;
-- and a patron can never be holding two drinks, so cashing is a single
-- unambiguous row. An expired open credit still occupies the slot, so the
-- grant re-uses the row (ON CONFLICT DO UPDATE) rather than inserting beside
-- it.
CREATE UNIQUE INDEX drink_credits_one_open_per_user
    ON drink_credits (user_id)
    WHERE cashed_at IS NULL;

-- The round's own roster: how many it bought and for whom.
CREATE INDEX drink_credits_round_idx ON drink_credits (round_id);
