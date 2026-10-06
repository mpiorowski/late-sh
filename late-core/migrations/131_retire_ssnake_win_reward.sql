-- Super Snake became a perpetual arena: there is no match to win any more,
-- so the cooldown win template is retired. Food and arena-clear payouts are
-- direct ledger writes (`ssnake_food`, `ssnake_arena_clear`, `ssnake_crash`)
-- with no reward template behind them.
UPDATE reward_templates
SET active = false,
    updated = current_timestamp
WHERE key = 'ssnake_win';
