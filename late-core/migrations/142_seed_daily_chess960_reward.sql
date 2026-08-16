-- Daily chess960 joins the daily-games roster: seed its win payout the same
-- way migrations 102/105/106/115/116/117/128 seeded the other daily games.
-- 500 chips, matching standard daily chess, paid once per match (per_event on
-- the match id).
INSERT INTO reward_templates
    (key, title, description, cadence, bucket, domain, difficulty, kind, params, target, reward_chips, weight, is_quest, claim_policy, cooldown_seconds)
VALUES
    ('daily_chess960_win_payout', 'Win Daily Chess960', 'Win a daily chess960 match from a shuffled back rank.', NULL, NULL, 'strategy', 'medium', 'game_win', '{"game":"daily_chess960","payout_kind":"win"}'::jsonb, 1, 500, 100, false, 'per_event', NULL);
