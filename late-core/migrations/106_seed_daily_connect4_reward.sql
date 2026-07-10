-- Daily connect four joins the daily-games roster: seed its win payout the
-- same way migrations 102/105 seeded daily chess and battleship. 400 chips,
-- paid once per match (per_event on the match id).
INSERT INTO reward_templates
    (key, title, description, cadence, bucket, domain, difficulty, kind, params, target, reward_chips, weight, is_quest, claim_policy, cooldown_seconds)
VALUES
    ('daily_connect4_win_payout', 'Win Daily Connect Four', 'Connect four in a daily connect four match.', NULL, NULL, 'strategy', 'medium', 'game_win', '{"game":"daily_connect4","payout_kind":"win"}'::jsonb, 1, 400, 100, false, 'per_event', NULL);
