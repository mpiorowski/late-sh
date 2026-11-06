-- Quartets is one wall a day for everyone, claimed like Word Wheel's wheel:
-- the first session of a UTC day inserts the date's row under an advisory
-- lock. puzzle_id names a pack puzzle. A puzzle may come round again once
-- the pack is used up, so puzzle_id is not unique here.
CREATE TABLE quartets_daily_puzzles (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    puzzle_date DATE NOT NULL UNIQUE,
    puzzle_id INT NOT NULL CHECK (puzzle_id > 0)
);

-- A player's tries on a day's wall, four word indices (0 to 15, the pack's
-- group order) per try, flattened. Everything else on the board replays
-- from them.
CREATE TABLE quartets_games (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    puzzle_date DATE NOT NULL,
    puzzle_id INT NOT NULL CHECK (puzzle_id > 0),
    tries SMALLINT[] NOT NULL DEFAULT '{}',
    UNIQUE(user_id, puzzle_date)
);

-- One row per solved wall. strikes is the misses spent on the way: three
-- end the wall unsolved, so a win has at most two.
CREATE TABLE quartets_daily_wins (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    puzzle_date DATE NOT NULL,
    strikes INT NOT NULL CHECK (strikes BETWEEN 0 AND 2),
    UNIQUE(user_id, puzzle_date)
);

-- The daily-win payout at the medium tier's Difficulty::chips, as Rubik's
-- Cube's. No quest templates: the quest pool is unchanged by this game.
INSERT INTO reward_templates
    (key, title, description, cadence, bucket, domain, difficulty, kind, params, target, reward_chips, weight, is_quest, claim_policy, cooldown_seconds)
VALUES
    ('quartets_daily_daily_win', 'Solve Quartets', 'Solve today''s Quartets wall.', NULL, NULL, 'puzzle', 'medium', 'daily_puzzle_win', '{"game":"quartets","difficulty":"daily","payout_kind":"daily_win"}'::jsonb, 1, 250, 100, false, 'utc_day', NULL)
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
