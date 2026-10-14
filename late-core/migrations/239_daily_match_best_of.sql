-- Pool and snooker challenges can be posted as a match of several frames:
-- best of 3, 5 or 7, raced to a majority. Chosen when the challenge is posted,
-- so it lives on the row (the open challenge has no game state yet) and is
-- copied into the pool state when the challenge is claimed. Every other game,
-- and every row from before this, is a single frame.
--
-- The win payout scales with it: the winner is paid the game's prize once per
-- frame they actually won (never less than once), so a match played out pays
-- for its length and a resignation one frame in pays what a single frame does.
ALTER TABLE daily_matches
    ADD COLUMN best_of SMALLINT NOT NULL DEFAULT 1
        CHECK (best_of IN (1, 3, 5, 7));
