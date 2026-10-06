-- Sliding Puzzle keeps one deterministic UTC-daily and one random personal
-- board per difficulty. Seeds let either mode restore its original scramble.
CREATE TABLE sliding_puzzle_games (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    mode VARCHAR NOT NULL CHECK (mode IN ('daily', 'personal')),
    difficulty_key VARCHAR NOT NULL CHECK (difficulty_key IN ('easy', 'medium', 'hard')),
    puzzle_date DATE,
    puzzle_seed BIGINT NOT NULL,
    tiles INT[] NOT NULL,
    moves INT NOT NULL DEFAULT 0 CHECK (moves >= 0),
    CHECK (
        (mode = 'daily' AND puzzle_date IS NOT NULL)
        OR (mode = 'personal' AND puzzle_date IS NULL)
    ),
    UNIQUE(user_id, difficulty_key, mode)
);

CREATE TABLE sliding_puzzle_daily_wins (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    difficulty_key VARCHAR NOT NULL CHECK (difficulty_key IN ('easy', 'medium', 'hard')),
    puzzle_date DATE NOT NULL,
    moves INT NOT NULL CHECK (moves > 0),
    UNIQUE(user_id, difficulty_key, puzzle_date)
);

INSERT INTO reward_templates
    (key, title, description, cadence, bucket, domain, difficulty, kind, params, target, reward_chips, weight, is_quest, claim_policy, cooldown_seconds)
VALUES
    ('sliding_puzzle_daily_easy_win', 'Solve easy Sliding Puzzle', 'Solve today''s easy Sliding Puzzle.', NULL, NULL, 'puzzle', 'easy', 'daily_puzzle_win', '{"game":"sliding_puzzle","difficulty":"easy","payout_kind":"daily_win_easy"}'::jsonb, 1, 100, 100, false, 'utc_day', NULL),
    ('sliding_puzzle_daily_medium_win', 'Solve medium Sliding Puzzle', 'Solve today''s medium Sliding Puzzle.', NULL, NULL, 'puzzle', 'medium', 'daily_puzzle_win', '{"game":"sliding_puzzle","difficulty":"medium","payout_kind":"daily_win_medium"}'::jsonb, 1, 250, 100, false, 'utc_day', NULL),
    ('sliding_puzzle_daily_hard_win', 'Solve hard Sliding Puzzle', 'Solve today''s hard Sliding Puzzle.', NULL, NULL, 'puzzle', 'hard', 'daily_puzzle_win', '{"game":"sliding_puzzle","difficulty":"hard","payout_kind":"daily_win_hard"}'::jsonb, 1, 500, 100, false, 'utc_day', NULL),
    ('solve_easy_sliding_puzzle', 'Solve easy Sliding Puzzle', 'Solve today''s easy Sliding Puzzle.', 'daily', 'quick', 'puzzle', 'easy', 'daily_puzzle_win', '{"game":"sliding_puzzle","difficulty":"easy"}'::jsonb, 1, 150, 100, true, 'assignment', NULL),
    ('solve_medium_sliding_puzzle', 'Solve medium Sliding Puzzle', 'Solve today''s medium Sliding Puzzle.', 'daily', 'skill', 'puzzle', 'medium', 'daily_puzzle_win', '{"game":"sliding_puzzle","difficulty":"medium"}'::jsonb, 1, 375, 100, true, 'assignment', NULL),
    ('solve_hard_sliding_puzzle', 'Solve hard Sliding Puzzle', 'Solve today''s hard Sliding Puzzle.', 'weekly', 'skill', 'puzzle', 'hard', 'daily_puzzle_win', '{"game":"sliding_puzzle","difficulty":"hard"}'::jsonb, 1, 750, 100, true, 'assignment', NULL)
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
