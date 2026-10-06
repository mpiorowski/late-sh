-- Daily reversi joins the daily-games roster: seed its win payout the same
-- way migrations 102/105/106 seeded chess, battleship, and connect four. 400
-- chips, paid once per match (per_event on the match id).
INSERT INTO reward_templates
    (key, title, description, cadence, bucket, domain, difficulty, kind, params, target, reward_chips, weight, is_quest, claim_policy, cooldown_seconds)
VALUES
    ('daily_reversi_win_payout', 'Win Daily Reversi', 'Hold the most discs in a daily reversi match.', NULL, NULL, 'strategy', 'medium', 'game_win', '{"game":"daily_reversi","payout_kind":"win"}'::jsonb, 1, 400, 100, false, 'per_event', NULL);
