-- Daily battleship joins the daily-games roster: seed its win payout the
-- same way migration 102 seeded daily chess. 300 chips, paid once per match
-- (per_event on the match id).
INSERT INTO reward_templates
    (key, title, description, cadence, bucket, domain, difficulty, kind, params, target, reward_chips, weight, is_quest, claim_policy, cooldown_seconds)
VALUES
    ('daily_battleship_win_payout', 'Win Daily Battleship', 'Sink the whole fleet in a daily battleship match.', NULL, NULL, 'strategy', 'medium', 'game_win', '{"game":"daily_battleship","payout_kind":"win"}'::jsonb, 1, 300, 100, false, 'per_event', NULL);
