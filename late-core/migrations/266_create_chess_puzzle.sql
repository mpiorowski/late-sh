-- Chess Puzzle keeps one UTC-daily and one personal board per difficulty,
-- as Netwalk does. The puzzle id names a pack entry; ply counts the solution
-- moves played after the opponent's setup move, so the board replays from
-- the pack's FEN. mistake_plies holds the ply of each wrong move: its length
-- is the lives spent, and the share card reads which moves were first try.
CREATE TABLE chess_puzzle_games (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    mode VARCHAR NOT NULL CHECK (mode IN ('daily', 'personal')),
    difficulty_key VARCHAR NOT NULL CHECK (difficulty_key IN ('easy', 'medium', 'hard')),
    puzzle_date DATE,
    puzzle_id VARCHAR NOT NULL,
    ply INT NOT NULL DEFAULT 0 CHECK (ply >= 0),
    mistake_plies INT[] NOT NULL DEFAULT '{}' CHECK (cardinality(mistake_plies) <= 3),
    CHECK (
        (mode = 'daily' AND puzzle_date IS NOT NULL)
        OR (mode = 'personal' AND puzzle_date IS NULL)
    ),
    UNIQUE(user_id, difficulty_key, mode)
);

CREATE TABLE chess_puzzle_daily_wins (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    difficulty_key VARCHAR NOT NULL CHECK (difficulty_key IN ('easy', 'medium', 'hard')),
    puzzle_date DATE NOT NULL,
    mistakes INT NOT NULL CHECK (mistakes BETWEEN 0 AND 2),
    UNIQUE(user_id, difficulty_key, puzzle_date)
);

-- The daily-win payouts, one per tier at that tier's Difficulty::chips.
-- No quest templates: the quest pool is unchanged by this game.
INSERT INTO reward_templates
    (key, title, description, cadence, bucket, domain, difficulty, kind, params, target, reward_chips, weight, is_quest, claim_policy, cooldown_seconds)
VALUES
    ('chess_puzzle_daily_easy_win', 'Solve easy Chess Puzzle', 'Find the moves in today''s easy Chess Puzzle.', NULL, NULL, 'puzzle', 'easy', 'daily_puzzle_win', '{"game":"chess_puzzle","difficulty":"easy","payout_kind":"daily_win_easy"}'::jsonb, 1, 100, 100, false, 'utc_day', NULL),
    ('chess_puzzle_daily_medium_win', 'Solve medium Chess Puzzle', 'Find the moves in today''s medium Chess Puzzle.', NULL, NULL, 'puzzle', 'medium', 'daily_puzzle_win', '{"game":"chess_puzzle","difficulty":"medium","payout_kind":"daily_win_medium"}'::jsonb, 1, 250, 100, false, 'utc_day', NULL),
    ('chess_puzzle_daily_hard_win', 'Solve hard Chess Puzzle', 'Find the moves in today''s hard Chess Puzzle.', NULL, NULL, 'puzzle', 'hard', 'daily_puzzle_win', '{"game":"chess_puzzle","difficulty":"hard","payout_kind":"daily_win_hard"}'::jsonb, 1, 500, 100, false, 'utc_day', NULL)
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
