-- Rubik's Cube and draw-3 Solitaire each live in two quest tiers so they can be
-- drawn as both a daily and a weekly quest. The daily slots draw easy (slot 1)
-- and medium (slot 2); the weekly slot draws hard. Rubik's already has a medium
-- daily template (`solve_rubiks_cube`, migration 088); add its hard weekly twin.
-- Solitaire already has a hard weekly draw-3 template (`win_draw_3_solitaire`,
-- migration 056); add its medium daily twin. Both share the same completion
-- params as their existing twin, so a single solve ticks both assignments on a
-- day where both are drawn.
INSERT INTO reward_templates
    (key, title, description, cadence, bucket, domain, difficulty, kind, params, target, reward_chips, weight, is_quest, claim_policy, cooldown_seconds)
VALUES
    (
        'solve_rubiks_cube_weekly',
        'Solve Rubik''s Cube',
        'Solve a Rubik''s Cube scramble any day this week.',
        'weekly',
        'skill',
        'arcade',
        'hard',
        'arcade_puzzle_solved',
        '{"game":"rubiks_cube","difficulty":"daily"}'::jsonb,
        1,
        750,
        100,
        true,
        'assignment',
        NULL
    ),
    (
        'win_draw_3_solitaire_daily',
        'Win draw-3 Solitaire',
        'Finish today''s draw-3 Solitaire deal.',
        'daily',
        'skill',
        'puzzle',
        'medium',
        'daily_puzzle_win',
        '{"game":"solitaire","difficulty":"draw-3"}'::jsonb,
        1,
        375,
        100,
        true,
        'assignment',
        NULL
    )
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
