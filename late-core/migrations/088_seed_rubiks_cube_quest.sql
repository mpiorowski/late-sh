INSERT INTO reward_templates
    (key, title, description, cadence, bucket, domain, difficulty, kind, params, target, reward_chips, weight, is_quest, claim_policy, cooldown_seconds)
VALUES
    ('rubiks_cube_daily_daily_win', 'Solve Rubik''s Cube', 'Solve today''s Rubik''s Cube scramble.', NULL, NULL, 'puzzle', 'medium', 'daily_puzzle_win', '{"game":"rubiks_cube","difficulty":"daily","payout_kind":"daily_win"}'::jsonb, 1, 250, 100, false, 'utc_day', NULL),
    ('solve_rubiks_cube', 'Solve Rubik''s Cube', 'Solve today''s Rubik''s Cube scramble.', 'daily', 'skill', 'arcade', 'medium', 'arcade_puzzle_solved', '{"game":"rubiks_cube","difficulty":"daily"}'::jsonb, 1, 375, 100, true, 'assignment', NULL)
ON CONFLICT (key) DO UPDATE SET
    title = EXCLUDED.title,
    description = EXCLUDED.description,
    cadence = EXCLUDED.cadence,
    bucket = EXCLUDED.bucket,
    domain = EXCLUDED.domain,
    difficulty = EXCLUDED.difficulty,
    kind = EXCLUDED.kind,
    params = EXCLUDED.params,
    target = EXCLUDED.target,
    reward_chips = EXCLUDED.reward_chips,
    weight = EXCLUDED.weight,
    is_quest = EXCLUDED.is_quest,
    claim_policy = EXCLUDED.claim_policy,
    cooldown_seconds = EXCLUDED.cooldown_seconds,
    active = true,
    updated = current_timestamp;
