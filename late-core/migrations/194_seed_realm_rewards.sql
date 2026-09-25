-- Realm ranked finish payouts, ~2000 chips total per game, tiered by the
-- player count at game start (see lobby/realm/svc.rs::payout_keys):
--   <= 3 players: winner takes all (2000)
--   4-6 players:  winner 1500, runner-up 500
--   7+ players:   winner 1250, runner-up 450, third 300
-- All per_event with event_key = game id, so each game pays each rank once.
INSERT INTO reward_templates
    (key, title, description, cadence, bucket, domain, difficulty, kind, params, target, reward_chips, weight, is_quest, claim_policy, cooldown_seconds)
VALUES
    ('realm_win_small', 'Conquer the Realm (small game)', 'Win a realm game of up to 3 players.', NULL, NULL, 'strategy', 'hard', 'game_win', '{"game":"realm","payout_kind":"win_small"}'::jsonb, 1, 2000, 100, false, 'per_event', NULL),
    ('realm_win_mid', 'Conquer the Realm', 'Win a realm game of 4-6 players.', NULL, NULL, 'strategy', 'hard', 'game_win', '{"game":"realm","payout_kind":"win_mid"}'::jsonb, 1, 1500, 100, false, 'per_event', NULL),
    ('realm_runnerup_mid', 'Realm Runner-up', 'Finish second in a realm game of 4-6 players.', NULL, NULL, 'strategy', 'hard', 'game_win', '{"game":"realm","payout_kind":"runnerup_mid"}'::jsonb, 1, 500, 100, false, 'per_event', NULL),
    ('realm_win_large', 'Conquer the Realm (grand game)', 'Win a realm game of 7 or more players.', NULL, NULL, 'strategy', 'hard', 'game_win', '{"game":"realm","payout_kind":"win_large"}'::jsonb, 1, 1250, 100, false, 'per_event', NULL),
    ('realm_runnerup_large', 'Realm Runner-up (grand game)', 'Finish second in a realm game of 7 or more players.', NULL, NULL, 'strategy', 'hard', 'game_win', '{"game":"realm","payout_kind":"runnerup_large"}'::jsonb, 1, 450, 100, false, 'per_event', NULL),
    ('realm_third_large', 'Realm Third Place', 'Finish third in a realm game of 7 or more players.', NULL, NULL, 'strategy', 'hard', 'game_win', '{"game":"realm","payout_kind":"third_large"}'::jsonb, 1, 300, 100, false, 'per_event', NULL);
