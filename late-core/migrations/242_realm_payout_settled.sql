-- What a finished realm's pot did, written on the row once the chips are
-- settled, the daily_matches.win_payout shape (migration 161). NULL is "not
-- settled yet": the finish is durable the moment the row flips, and the
-- credits run afterwards, so a crash or a failed credit between the two used
-- to lose the pot for good while the results card still showed it as paid.
-- The sweeper now settles every finished row whose payout is NULL; the
-- per-event claims on game_payout_claims keep a retry from paying twice.
--   paid       every place in the plan was credited (or already had been)
--   unearned   a winner, but the game was too short or too small to pay
--   dissolved  no winner, nothing to pay
ALTER TABLE realm_games
    ADD COLUMN payout TEXT
        CHECK (payout IN ('paid', 'unearned', 'dissolved'));
