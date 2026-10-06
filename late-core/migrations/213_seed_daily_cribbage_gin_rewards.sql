-- Daily cribbage and gin rummy join the daily-games roster: seed their win
-- payouts the same way migrations 102-178 seeded the others. 500 chips each
-- rather than the board games' 400: both are played over several hands, to 61
-- and to 100. Paid once per match (per_event on the match id).
INSERT INTO reward_templates
    (key, title, description, cadence, bucket, domain, difficulty, kind, params, target, reward_chips, weight, is_quest, claim_policy, cooldown_seconds)
VALUES
    ('daily_cribbage_win_payout', 'Win Daily Cribbage', 'Peg out first: reach 61 in a daily cribbage match.', NULL, NULL, 'strategy', 'medium', 'game_win', '{"game":"daily_cribbage","payout_kind":"win"}'::jsonb, 1, 500, 100, false, 'per_event', NULL),
    ('daily_gin_win_payout', 'Win Daily Gin Rummy', 'Reach 100 first in a daily gin rummy match.', NULL, NULL, 'strategy', 'medium', 'game_win', '{"game":"daily_gin","payout_kind":"win"}'::jsonb, 1, 500, 100, false, 'per_event', NULL);
