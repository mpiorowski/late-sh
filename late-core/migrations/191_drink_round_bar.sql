-- Which bar sold the round. Two things read it.
--
-- The Nightcap's tab board (late-ssh/src/app/clubhouse/nightcap) is the
-- house's own leaderboard: it counts rounds bought at the Nightcap and
-- nothing else. A tavern round reaches everyone online, so at the same
-- price a head it costs an order of magnitude more and would own the board
-- forever.
--
-- And the buzz a credit cashes for. A tavern round buys for everyone
-- online, most of whom will never walk up, so the drinks that do land pour
-- more than the buyer paid a head; a Nightcap round buys for the stools,
-- who are sitting right there and will drink, so it pours chips to points
-- 1:1 like every other drink at that bar
-- (late_core::models::drink_round::Bar::drink_points).
--
-- Rounds bought before this column stay 'tavern': the ledger never said
-- which bar sold them, and the tavern is where the mechanic started.
ALTER TABLE drink_rounds ADD COLUMN bar TEXT NOT NULL DEFAULT 'tavern'
    CHECK (bar IN ('tavern', 'nightcap'));

-- Every round names its bar from here on; nothing falls back to a default.
ALTER TABLE drink_rounds ALTER COLUMN bar DROP DEFAULT;
