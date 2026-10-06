-- Two payout templates outlived their games. `chess_win_payout` belonged to
-- the rooms-era chess table, which the Lobby consolidation replaced with the
-- daily correspondence match (`daily_chess_win_payout`); `sshattrick_win_payout`
-- belonged to ssHattrick, which is gone entirely. Neither key is referenced by
-- any code path, so they can never pay again, but they still show up as active
-- rows to anything reading the roster.
--
-- Deactivate, never delete (same call as migration 110): past payouts and
-- ledger rows reference these templates, and dropping them would orphan that
-- history. They are not quests, so there are no assignments to clean up.
UPDATE reward_templates
SET active = false,
    updated = current_timestamp
WHERE key IN ('chess_win_payout', 'sshattrick_win_payout');
