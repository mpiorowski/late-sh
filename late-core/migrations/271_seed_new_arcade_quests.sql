-- Quest templates for the dailies that shipped without them (265 to 268).
-- The keyed games follow Sliding Puzzle: easy is a quick daily, medium a
-- skill daily, hard a weekly. Quartets has one wall a day, so it follows
-- Rubik's Cube: a medium daily and a hard weekly twin sharing the same
-- params, so one solve ticks both when both are drawn.
INSERT INTO reward_templates
    (key, title, description, cadence, bucket, domain, difficulty, kind, params, target, reward_chips, weight, is_quest, claim_policy, cooldown_seconds)
VALUES
    ('solve_easy_netwalk', 'Solve easy Netwalk', 'Connect today''s easy Netwalk.', 'daily', 'quick', 'puzzle', 'easy', 'daily_puzzle_win', '{"game":"netwalk","difficulty":"easy"}'::jsonb, 1, 150, 100, true, 'assignment', NULL),
    ('solve_medium_netwalk', 'Solve medium Netwalk', 'Connect today''s medium Netwalk.', 'daily', 'skill', 'puzzle', 'medium', 'daily_puzzle_win', '{"game":"netwalk","difficulty":"medium"}'::jsonb, 1, 375, 100, true, 'assignment', NULL),
    ('solve_hard_netwalk', 'Solve hard Netwalk', 'Connect a hard Netwalk any day this week.', 'weekly', 'skill', 'puzzle', 'hard', 'daily_puzzle_win', '{"game":"netwalk","difficulty":"hard"}'::jsonb, 1, 750, 100, true, 'assignment', NULL),
    ('solve_easy_chess_puzzle', 'Solve easy Chess Puzzle', 'Find the moves in today''s easy Chess Puzzle.', 'daily', 'quick', 'puzzle', 'easy', 'daily_puzzle_win', '{"game":"chess_puzzle","difficulty":"easy"}'::jsonb, 1, 150, 100, true, 'assignment', NULL),
    ('solve_medium_chess_puzzle', 'Solve medium Chess Puzzle', 'Find the moves in today''s medium Chess Puzzle.', 'daily', 'skill', 'puzzle', 'medium', 'daily_puzzle_win', '{"game":"chess_puzzle","difficulty":"medium"}'::jsonb, 1, 375, 100, true, 'assignment', NULL),
    ('solve_hard_chess_puzzle', 'Solve hard Chess Puzzle', 'Find the moves in a hard Chess Puzzle any day this week.', 'weekly', 'skill', 'puzzle', 'hard', 'daily_puzzle_win', '{"game":"chess_puzzle","difficulty":"hard"}'::jsonb, 1, 750, 100, true, 'assignment', NULL),
    ('reach_bronze_word_wheel', 'Reach Bronze on Word Wheel', 'Reach Bronze on today''s Word Wheel.', 'daily', 'quick', 'puzzle', 'easy', 'daily_puzzle_win', '{"game":"word_wheel","difficulty":"easy"}'::jsonb, 1, 150, 100, true, 'assignment', NULL),
    ('reach_silver_word_wheel', 'Reach Silver on Word Wheel', 'Reach Silver on today''s Word Wheel.', 'daily', 'skill', 'puzzle', 'medium', 'daily_puzzle_win', '{"game":"word_wheel","difficulty":"medium"}'::jsonb, 1, 375, 100, true, 'assignment', NULL),
    ('reach_gold_word_wheel', 'Reach Gold on Word Wheel', 'Reach Gold on a Word Wheel any day this week.', 'weekly', 'skill', 'puzzle', 'hard', 'daily_puzzle_win', '{"game":"word_wheel","difficulty":"hard"}'::jsonb, 1, 750, 100, true, 'assignment', NULL),
    ('solve_quartets', 'Solve Quartets', 'Solve today''s Quartets wall.', 'daily', 'skill', 'puzzle', 'medium', 'daily_puzzle_win', '{"game":"quartets","difficulty":"daily"}'::jsonb, 1, 375, 100, true, 'assignment', NULL),
    ('solve_quartets_weekly', 'Solve Quartets', 'Solve a Quartets wall any day this week.', 'weekly', 'skill', 'puzzle', 'hard', 'daily_puzzle_win', '{"game":"quartets","difficulty":"daily"}'::jsonb, 1, 750, 100, true, 'assignment', NULL)
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
