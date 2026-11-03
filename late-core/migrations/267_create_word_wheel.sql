-- Word Wheel is one wheel a day for everyone, claimed like Le Word's word:
-- the first session of a UTC day inserts the date's row under an advisory
-- lock. letters names a pack wheel (the hub, then the rim). A wheel may come
-- round again once the pack is used up, so letters is not unique here.
CREATE TABLE word_wheel_daily_wheels (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    puzzle_date DATE NOT NULL UNIQUE,
    letters VARCHAR NOT NULL CHECK (letters ~ '^[a-z]{7}$')
);

-- A player's words on a day's wheel. found only grows, so a save merges
-- into what is stored and two sessions never drop each other's words.
CREATE TABLE word_wheel_games (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    puzzle_date DATE NOT NULL,
    letters VARCHAR NOT NULL CHECK (letters ~ '^[a-z]{7}$'),
    found TEXT[] NOT NULL DEFAULT '{}',
    UNIQUE(user_id, puzzle_date)
);

-- One row per rank reached: easy is Bronze, medium Silver, hard Gold, all
-- on the same wheel. score is the wheel score when the rank was reached.
CREATE TABLE word_wheel_daily_wins (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    difficulty_key VARCHAR NOT NULL CHECK (difficulty_key IN ('easy', 'medium', 'hard')),
    puzzle_date DATE NOT NULL,
    score INT NOT NULL CHECK (score > 0),
    UNIQUE(user_id, difficulty_key, puzzle_date)
);

-- The daily-win payouts, one per rank at that tier's Difficulty::chips.
-- No quest templates: the quest pool is unchanged by this game.
INSERT INTO reward_templates
    (key, title, description, cadence, bucket, domain, difficulty, kind, params, target, reward_chips, weight, is_quest, claim_policy, cooldown_seconds)
VALUES
    ('word_wheel_daily_easy_win', 'Reach Bronze on Word Wheel', 'Reach Bronze on today''s Word Wheel.', NULL, NULL, 'puzzle', 'easy', 'daily_puzzle_win', '{"game":"word_wheel","difficulty":"easy","payout_kind":"daily_win_easy"}'::jsonb, 1, 100, 100, false, 'utc_day', NULL),
    ('word_wheel_daily_medium_win', 'Reach Silver on Word Wheel', 'Reach Silver on today''s Word Wheel.', NULL, NULL, 'puzzle', 'medium', 'daily_puzzle_win', '{"game":"word_wheel","difficulty":"medium","payout_kind":"daily_win_medium"}'::jsonb, 1, 250, 100, false, 'utc_day', NULL),
    ('word_wheel_daily_hard_win', 'Reach Gold on Word Wheel', 'Reach Gold on today''s Word Wheel.', NULL, NULL, 'puzzle', 'hard', 'daily_puzzle_win', '{"game":"word_wheel","difficulty":"hard","payout_kind":"daily_win_hard"}'::jsonb, 1, 500, 100, false, 'utc_day', NULL)
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
