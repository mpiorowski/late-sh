CREATE TABLE le_word_daily_words (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    puzzle_date DATE NOT NULL UNIQUE,
    answer_word TEXT NOT NULL CHECK (answer_word ~ '^[a-z]{5}$'),
    UNIQUE(answer_word)
);

CREATE INDEX le_word_daily_words_created_idx
    ON le_word_daily_words (created DESC);

CREATE TABLE le_word_games (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    puzzle_date DATE NOT NULL,
    answer_word TEXT NOT NULL CHECK (answer_word ~ '^[a-z]{5}$'),
    guesses JSONB NOT NULL DEFAULT '[]'::jsonb,
    current_guess TEXT NOT NULL DEFAULT '',
    is_game_over BOOLEAN NOT NULL DEFAULT false,
    won BOOLEAN NOT NULL DEFAULT false,
    UNIQUE(user_id, puzzle_date)
);

CREATE INDEX le_word_games_user_updated_idx
    ON le_word_games (user_id, updated DESC);

CREATE TABLE le_word_daily_wins (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    puzzle_date DATE NOT NULL,
    score INT NOT NULL CHECK (score BETWEEN 1 AND 6),
    UNIQUE(user_id, puzzle_date)
);

INSERT INTO reward_templates
    (key, title, description, cadence, bucket, domain, difficulty, kind, params, target, reward_chips, weight, is_quest, claim_policy, cooldown_seconds)
VALUES
    ('le_word_daily_daily_win', 'Solve Le Word', 'Solve today''s Le Word.', NULL, NULL, 'puzzle', NULL, 'daily_puzzle_win', '{"game":"le_word","difficulty":"daily","payout_kind":"daily_win"}'::jsonb, 1, 100, 100, false, 'utc_day', NULL),
    ('solve_le_word', 'Solve Le Word', 'Solve today''s Le Word.', 'daily', 'quick', 'puzzle', 'easy', 'daily_puzzle_win', '{"game":"le_word","difficulty":"daily"}'::jsonb, 1, 150, 100, true, 'assignment', NULL)
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
