-- Daily snooker joins the daily-games roster: seed its win payout the same
-- way migrations 102/105/106/115/116/117/128/176/177 seeded the others.
-- 700 chips rather than the pool games' 400: a frame is fifteen reds, six
-- colours and a scoreboard, and the longest match on the roster by a distance.
-- Paid once per match (per_event on the match id).
INSERT INTO reward_templates
    (key, title, description, cadence, bucket, domain, difficulty, kind, params, target, reward_chips, weight, is_quest, claim_policy, cooldown_seconds)
VALUES
    ('daily_snooker_win_payout', 'Win Daily Snooker', 'Finish a daily snooker frame in front: reds and colours, then the colours in order.', NULL, NULL, 'strategy', 'hard', 'game_win', '{"game":"daily_snooker","payout_kind":"win"}'::jsonb, 1, 700, 100, false, 'per_event', NULL);
