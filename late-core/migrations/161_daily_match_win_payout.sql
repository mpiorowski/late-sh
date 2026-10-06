-- What a daily match's win payout did (SHOP.md Phase 7 gates): paid, or refused
-- because the match held too few moves or a win against that opponent from the
-- same posting day already paid, or the credit call failed. The finish banner
-- reaches only a connected winner; the lingering result row reads this column
-- so an offline winner learns why the chips did or did not come.
--
-- NULL for draws and for matches finished before the gates existed. Written by
-- a second UPDATE after the finish, on purpose: the payout is decided once the
-- finish is durable, and a failed credit must never un-finish a match.
--
-- Numbered 161: 160 is reserved for the pot (SHOP.md Phase 5), in flight in a
-- parallel session.
ALTER TABLE daily_matches
    ADD COLUMN win_payout TEXT
        CHECK (win_payout IN ('paid', 'unplayed', 'pair_day_capped', 'failed'));
