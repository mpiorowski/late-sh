CREATE TABLE quest_templates (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    key TEXT NOT NULL UNIQUE,
    title TEXT NOT NULL,
    description TEXT NOT NULL,
    cadence TEXT NOT NULL CHECK (cadence IN ('daily', 'weekly')),
    bucket TEXT NOT NULL,
    domain TEXT NOT NULL,
    difficulty TEXT NOT NULL CHECK (difficulty IN ('easy', 'medium', 'hard')),
    kind TEXT NOT NULL,
    params JSONB NOT NULL DEFAULT '{}'::jsonb,
    target INT NOT NULL CHECK (target > 0),
    reward_chips BIGINT NOT NULL DEFAULT 0 CHECK (reward_chips >= 0),
    weight INT NOT NULL DEFAULT 100 CHECK (weight > 0),
    active BOOLEAN NOT NULL DEFAULT true,
    starts_at TIMESTAMPTZ,
    ends_at TIMESTAMPTZ
);

CREATE INDEX quest_templates_draw_idx
    ON quest_templates (cadence, active, bucket, weight);

CREATE TABLE quest_assignments (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    cadence TEXT NOT NULL CHECK (cadence IN ('daily', 'weekly')),
    period_start DATE NOT NULL,
    period_end DATE NOT NULL,
    slot INT NOT NULL CHECK (slot > 0),
    template_id UUID NOT NULL REFERENCES quest_templates(id) ON DELETE RESTRICT,
    UNIQUE (cadence, period_start, slot)
);

CREATE INDEX quest_assignments_active_idx
    ON quest_assignments (period_start, period_end, cadence, slot);

CREATE TABLE user_quest_progress (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    assignment_id UUID NOT NULL REFERENCES quest_assignments(id) ON DELETE CASCADE,
    progress INT NOT NULL DEFAULT 0 CHECK (progress >= 0),
    completed_at TIMESTAMPTZ,
    rewarded_at TIMESTAMPTZ,
    UNIQUE (user_id, assignment_id)
);

CREATE INDEX user_quest_progress_user_idx
    ON user_quest_progress (user_id, updated DESC);

CREATE TABLE quest_progress_events (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    assignment_id UUID NOT NULL REFERENCES quest_assignments(id) ON DELETE CASCADE,
    event_id UUID NOT NULL,
    amount INT NOT NULL,
    UNIQUE (assignment_id, event_id)
);

CREATE INDEX quest_progress_events_user_idx
    ON quest_progress_events (user_id, created DESC);

INSERT INTO quest_templates
    (key, title, description, cadence, bucket, domain, difficulty, kind, params, target, reward_chips, weight)
VALUES
    ('cast_music_vote', 'Cast a music vote', 'Vote for the next station mood.', 'daily', 'quick', 'social', 'easy', 'vote_cast', '{}'::jsonb, 1, 75, 130),
    ('water_bonsai', 'Water your bonsai', 'Give your bonsai its daily care.', 'daily', 'quick', 'bonsai', 'easy', 'bonsai_watered', '{}'::jsonb, 1, 75, 130),
    ('play_3_blackjack_hands', 'Play 3 Blackjack hands', 'Sit at a table and finish 3 Blackjack hands.', 'daily', 'quick', 'casino', 'easy', 'room_rounds_played', '{"game":"blackjack"}'::jsonb, 3, 100, 90),
    ('play_3_poker_hands', 'Play 3 Poker hands', 'Finish 3 Poker hands with chips committed.', 'daily', 'quick', 'casino', 'easy', 'room_rounds_played', '{"game":"poker"}'::jsonb, 3, 100, 90),
    ('win_easy_sudoku', 'Win easy Sudoku', 'Solve today''s easy Sudoku.', 'daily', 'quick', 'puzzle', 'easy', 'daily_puzzle_win', '{"game":"sudoku","difficulty":"easy"}'::jsonb, 1, 100, 90),
    ('win_medium_sudoku', 'Win medium Sudoku', 'Solve today''s medium Sudoku.', 'daily', 'skill', 'puzzle', 'medium', 'daily_puzzle_win', '{"game":"sudoku","difficulty":"medium"}'::jsonb, 1, 175, 110),
    ('clear_medium_minesweeper', 'Clear medium Minesweeper', 'Clear today''s medium Minesweeper board.', 'daily', 'skill', 'puzzle', 'medium', 'daily_puzzle_win', '{"game":"minesweeper","difficulty":"medium"}'::jsonb, 1, 175, 90),
    ('win_draw_1_solitaire', 'Win draw-1 Solitaire', 'Finish today''s draw-1 Solitaire deal.', 'daily', 'skill', 'puzzle', 'medium', 'daily_puzzle_win', '{"game":"solitaire","difficulty":"draw-1"}'::jsonb, 1, 175, 90),
    ('score_1500_tetris', 'Score 1,500 in Tetris', 'Finish a Tetris run with at least 1,500 points.', 'daily', 'skill', 'arcade', 'medium', 'arcade_score', '{"game":"tetris"}'::jsonb, 1500, 150, 90),
    ('reach_snake_level_3', 'Reach Snake level 3', 'Finish a Snake run after reaching level 3.', 'daily', 'skill', 'arcade', 'medium', 'arcade_level', '{"game":"snake"}'::jsonb, 3, 150, 90),
    ('win_hard_sudoku', 'Win hard Sudoku', 'Solve today''s hard Sudoku.', 'weekly', 'skill', 'puzzle', 'hard', 'daily_puzzle_win', '{"game":"sudoku","difficulty":"hard"}'::jsonb, 1, 650, 110),
    ('clear_hard_minesweeper', 'Clear hard Minesweeper', 'Clear today''s hard Minesweeper board.', 'weekly', 'skill', 'puzzle', 'hard', 'daily_puzzle_win', '{"game":"minesweeper","difficulty":"hard"}'::jsonb, 1, 650, 90),
    ('win_draw_3_solitaire', 'Win draw-3 Solitaire', 'Finish today''s draw-3 Solitaire deal.', 'weekly', 'skill', 'puzzle', 'hard', 'daily_puzzle_win', '{"game":"solitaire","difficulty":"draw-3"}'::jsonb, 1, 650, 90),
    ('score_10000_tetris', 'Score 10,000 in Tetris', 'Finish a Tetris run with at least 10,000 points.', 'weekly', 'skill', 'arcade', 'hard', 'arcade_score', '{"game":"tetris"}'::jsonb, 10000, 600, 90),
    ('reach_snake_level_7', 'Reach Snake level 7', 'Finish a Snake run after reaching level 7.', 'weekly', 'skill', 'arcade', 'hard', 'arcade_level', '{"game":"snake"}'::jsonb, 7, 600, 90),
    ('play_10_blackjack_hands', 'Play 10 Blackjack hands', 'Finish 10 Blackjack hands this week.', 'weekly', 'casino', 'casino', 'medium', 'room_rounds_played', '{"game":"blackjack"}'::jsonb, 10, 450, 100),
    ('play_10_poker_hands', 'Play 10 Poker hands', 'Finish 10 Poker hands this week.', 'weekly', 'casino', 'casino', 'medium', 'room_rounds_played', '{"game":"poker"}'::jsonb, 10, 450, 100),
    ('win_3_blackjack_hands', 'Win 3 Blackjack hands', 'Win 3 Blackjack hands this week.', 'weekly', 'casino', 'casino', 'hard', 'room_wins', '{"game":"blackjack"}'::jsonb, 3, 550, 80),
    ('win_3_poker_hands', 'Win 3 Poker hands', 'Win 3 Poker hands this week.', 'weekly', 'casino', 'casino', 'hard', 'room_wins', '{"game":"poker"}'::jsonb, 3, 550, 80)
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
    active = true,
    updated = current_timestamp;
