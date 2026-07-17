-- Tron win payout is a flat 100 chips regardless of rider count; the
-- per-rider-count keys stay so existing payout params keep matching.
UPDATE reward_templates
SET reward_chips = 100,
    updated = current_timestamp
WHERE key IN ('tron_win_2p', 'tron_win_3p', 'tron_win_4p');
