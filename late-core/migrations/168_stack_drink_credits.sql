-- The round, second cut: a patron banks every round they were not around to
-- drink, up to a cap (SHOP.md, "The round").
--
-- Migration 164 allowed exactly one open credit per patron, ever. That index
-- was doing two jobs: keeping a patron from holding two drinks at once, and
-- throttling the mechanic, since a second round moments after the first
-- reached nobody and was refused uncharged. The first job was never the point.
-- A patron who was mid-game through three rounds ended the night owed exactly
-- one drink, and the two buyers who paid for the others bought nothing.
--
-- Credits stack now, capped at `MAX_OPEN_CREDITS`
-- (`late-core/src/models/drink_round.rs`). The cap moves out of the index and
-- into the grant, which counts a patron's open, unexpired credits and skips
-- anyone already at it: an index can only say "one", and the number of drinks
-- a patron may bank is a tuning dial, not a schema fact. The throttle the old
-- index provided is now that cap.
--
-- What the schema still has to guarantee is that one round credits one patron
-- exactly once, which is what makes the grant's ON CONFLICT DO NOTHING a
-- no-op rather than a second drink.
--
-- Deploy note: there is no order in which this migration and the code swap
-- both work. The old binary's grant infers its arbiter from the index dropped
-- here, so once this runs, every round purchase on old code errors (uncharged)
-- until the new binary is up; the new binary's ON CONFLICT (round_id, user_id)
-- needs the index created here. Apply and roll out back to back and accept
-- that rounds are down for the gap.
DROP INDEX drink_credits_one_open_per_user;

CREATE UNIQUE INDEX drink_credits_one_per_round_per_user
    ON drink_credits (round_id, user_id);

-- Both hot reads are "this patron's open credits, soonest to expire first":
-- the grant's cap count, and the pour, which spends the one closest to going
-- cold. Uncashed rows still die on their own and nothing sweeps them; an
-- expired one is simply not counted rather than re-used in place.
CREATE INDEX drink_credits_open_by_user
    ON drink_credits (user_id, expires_at)
    WHERE cashed_at IS NULL;

-- Redundant now: the round-and-patron unique index above answers every
-- lookup by round on its leading column.
DROP INDEX drink_credits_round_idx;
