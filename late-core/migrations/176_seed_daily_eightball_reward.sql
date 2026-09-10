-- Daily eight-ball joins the daily-games roster: seed its win payout the same
-- way migrations 102/105/106/115/116/117/128 seeded the other daily games.
-- 400 chips, paid once per match (per_event on the match id).
INSERT INTO reward_templates
    (key, title, description, cadence, bucket, domain, difficulty, kind, params, target, reward_chips, weight, is_quest, claim_policy, cooldown_seconds)
VALUES
    ('daily_eightball_win_payout', 'Win Daily Eight-Ball', 'Clear your group and sink the eight in the pocket you called.', NULL, NULL, 'strategy', 'medium', 'game_win', '{"game":"daily_eightball","payout_kind":"win"}'::jsonb, 1, 400, 100, false, 'per_event', NULL);
