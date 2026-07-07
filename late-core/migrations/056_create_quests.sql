CREATE TABLE reward_templates (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    key TEXT NOT NULL UNIQUE,
    title TEXT NOT NULL,
    description TEXT NOT NULL,
    cadence TEXT CHECK (cadence IS NULL OR cadence IN ('daily', 'weekly')),
    bucket TEXT,
    domain TEXT NOT NULL,
    difficulty TEXT CHECK (difficulty IS NULL OR difficulty IN ('easy', 'medium', 'hard')),
    kind TEXT NOT NULL,
    params JSONB NOT NULL DEFAULT '{}'::jsonb,
    target INT NOT NULL CHECK (target > 0),
    reward_chips BIGINT NOT NULL DEFAULT 0 CHECK (reward_chips >= 0),
    weight INT NOT NULL DEFAULT 100 CHECK (weight > 0),
    is_quest BOOLEAN NOT NULL DEFAULT false,
    claim_policy TEXT NOT NULL DEFAULT 'assignment'
        CHECK (claim_policy IN ('assignment', 'utc_day', 'cooldown', 'per_event')),
    cooldown_seconds INT CHECK (cooldown_seconds IS NULL OR cooldown_seconds > 0),
    active BOOLEAN NOT NULL DEFAULT true,
    starts_at TIMESTAMPTZ,
    ends_at TIMESTAMPTZ,
    CHECK (is_quest = false OR cadence IS NOT NULL),
    CHECK (claim_policy != 'cooldown' OR cooldown_seconds IS NOT NULL),
    CHECK (claim_policy = 'cooldown' OR cooldown_seconds IS NULL)
);

CREATE INDEX reward_templates_quest_draw_idx
    ON reward_templates (cadence, active, bucket, weight)
    WHERE is_quest = true;

CREATE TABLE quest_assignments (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    cadence TEXT NOT NULL CHECK (cadence IN ('daily', 'weekly')),
    period_start DATE NOT NULL,
    period_end DATE NOT NULL,
    slot INT NOT NULL CHECK (slot > 0),
    template_id UUID NOT NULL REFERENCES reward_templates(id) ON DELETE RESTRICT,
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

INSERT INTO reward_templates
    (key, title, description, cadence, bucket, domain, difficulty, kind, params, target, reward_chips, weight, is_quest, claim_policy, cooldown_seconds)
VALUES
    ('sudoku_daily_easy_win', 'Win easy Sudoku', 'Solve today''s easy Sudoku.', NULL, NULL, 'puzzle', 'easy', 'daily_puzzle_win', '{"game":"sudoku","difficulty":"easy","payout_kind":"daily_win_easy"}'::jsonb, 1, 100, 100, false, 'utc_day', NULL),
    ('sudoku_daily_medium_win', 'Win medium Sudoku', 'Solve today''s medium Sudoku.', NULL, NULL, 'puzzle', 'medium', 'daily_puzzle_win', '{"game":"sudoku","difficulty":"medium","payout_kind":"daily_win_medium"}'::jsonb, 1, 250, 100, false, 'utc_day', NULL),
    ('sudoku_daily_hard_win', 'Win hard Sudoku', 'Solve today''s hard Sudoku.', NULL, NULL, 'puzzle', 'hard', 'daily_puzzle_win', '{"game":"sudoku","difficulty":"hard","payout_kind":"daily_win_hard"}'::jsonb, 1, 500, 100, false, 'utc_day', NULL),
    ('nonogram_daily_easy_win', 'Solve easy Nonogram', 'Solve today''s easy Nonogram.', NULL, NULL, 'puzzle', 'easy', 'daily_puzzle_win', '{"game":"nonogram","difficulty":"easy","payout_kind":"daily_win_easy"}'::jsonb, 1, 100, 100, false, 'utc_day', NULL),
    ('nonogram_daily_medium_win', 'Solve medium Nonogram', 'Solve today''s medium Nonogram.', NULL, NULL, 'puzzle', 'medium', 'daily_puzzle_win', '{"game":"nonogram","difficulty":"medium","payout_kind":"daily_win_medium"}'::jsonb, 1, 250, 100, false, 'utc_day', NULL),
    ('nonogram_daily_hard_win', 'Solve hard Nonogram', 'Solve today''s hard Nonogram.', NULL, NULL, 'puzzle', 'hard', 'daily_puzzle_win', '{"game":"nonogram","difficulty":"hard","payout_kind":"daily_win_hard"}'::jsonb, 1, 500, 100, false, 'utc_day', NULL),
    ('minesweeper_daily_easy_win', 'Clear easy Minesweeper', 'Clear today''s easy Minesweeper board.', NULL, NULL, 'puzzle', 'easy', 'daily_puzzle_win', '{"game":"minesweeper","difficulty":"easy","payout_kind":"daily_win_easy"}'::jsonb, 1, 100, 100, false, 'utc_day', NULL),
    ('minesweeper_daily_medium_win', 'Clear medium Minesweeper', 'Clear today''s medium Minesweeper board.', NULL, NULL, 'puzzle', 'medium', 'daily_puzzle_win', '{"game":"minesweeper","difficulty":"medium","payout_kind":"daily_win_medium"}'::jsonb, 1, 250, 100, false, 'utc_day', NULL),
    ('minesweeper_daily_hard_win', 'Clear hard Minesweeper', 'Clear today''s hard Minesweeper board.', NULL, NULL, 'puzzle', 'hard', 'daily_puzzle_win', '{"game":"minesweeper","difficulty":"hard","payout_kind":"daily_win_hard"}'::jsonb, 1, 500, 100, false, 'utc_day', NULL),
    ('solitaire_daily_draw_1_win', 'Win draw-1 Solitaire', 'Finish today''s draw-1 Solitaire deal.', NULL, NULL, 'puzzle', 'medium', 'daily_puzzle_win', '{"game":"solitaire","difficulty":"draw-1","payout_kind":"daily_win_draw_1"}'::jsonb, 1, 250, 100, false, 'utc_day', NULL),
    ('solitaire_daily_draw_3_win', 'Win draw-3 Solitaire', 'Finish today''s draw-3 Solitaire deal.', NULL, NULL, 'puzzle', 'hard', 'daily_puzzle_win', '{"game":"solitaire","difficulty":"draw-3","payout_kind":"daily_win_draw_3"}'::jsonb, 1, 500, 100, false, 'utc_day', NULL),
    ('asterion_daily_escape', 'Escape Asterion', 'Escape the final Asterion maze.', NULL, NULL, 'maze', 'hard', 'game_win', '{"game":"asterion","payout_kind":"escape"}'::jsonb, 1, 4000, 100, false, 'utc_day', NULL),
    ('chess_win_payout', 'Win Chess', 'Win a decisive Chess game.', NULL, NULL, 'strategy', 'hard', 'game_win', '{"game":"chess","payout_kind":"win"}'::jsonb, 1, 500, 100, false, 'cooldown', 3600),
    ('tron_win_2p', 'Win 2-player Tron', 'Win a Tron round that started with 2 riders.', NULL, NULL, 'arcade', 'medium', 'game_win', '{"game":"tron","payout_kind":"win","rider_count":2}'::jsonb, 1, 50, 100, false, 'cooldown', 300),
    ('tron_win_3p', 'Win 3-player Tron', 'Win a Tron round that started with 3 riders.', NULL, NULL, 'arcade', 'medium', 'game_win', '{"game":"tron","payout_kind":"win","rider_count":3}'::jsonb, 1, 75, 100, false, 'cooldown', 300),
    ('tron_win_4p', 'Win 4-player Tron', 'Win a Tron round that started with 4 riders.', NULL, NULL, 'arcade', 'medium', 'game_win', '{"game":"tron","payout_kind":"win","rider_count":4}'::jsonb, 1, 100, 100, false, 'cooldown', 300),
    ('win_easy_sudoku', 'Win easy Sudoku', 'Solve today''s easy Sudoku.', 'daily', 'quick', 'puzzle', 'easy', 'daily_puzzle_win', '{"game":"sudoku","difficulty":"easy"}'::jsonb, 1, 150, 100, true, 'assignment', NULL),
    ('win_medium_sudoku', 'Win medium Sudoku', 'Solve today''s medium Sudoku.', 'daily', 'skill', 'puzzle', 'medium', 'daily_puzzle_win', '{"game":"sudoku","difficulty":"medium"}'::jsonb, 1, 375, 100, true, 'assignment', NULL),
    ('solve_easy_nonogram', 'Solve easy Nonogram', 'Solve today''s easy Nonogram.', 'daily', 'quick', 'puzzle', 'easy', 'daily_puzzle_win', '{"game":"nonogram","difficulty":"easy"}'::jsonb, 1, 150, 100, true, 'assignment', NULL),
    ('solve_medium_nonogram', 'Solve medium Nonogram', 'Solve today''s medium Nonogram.', 'daily', 'skill', 'puzzle', 'medium', 'daily_puzzle_win', '{"game":"nonogram","difficulty":"medium"}'::jsonb, 1, 375, 100, true, 'assignment', NULL),
    ('clear_easy_minesweeper', 'Clear easy Minesweeper', 'Clear today''s easy Minesweeper board.', 'daily', 'quick', 'puzzle', 'easy', 'daily_puzzle_win', '{"game":"minesweeper","difficulty":"easy"}'::jsonb, 1, 150, 100, true, 'assignment', NULL),
    ('clear_medium_minesweeper', 'Clear medium Minesweeper', 'Clear today''s medium Minesweeper board.', 'daily', 'skill', 'puzzle', 'medium', 'daily_puzzle_win', '{"game":"minesweeper","difficulty":"medium"}'::jsonb, 1, 375, 100, true, 'assignment', NULL),
    ('win_draw_1_solitaire', 'Win draw-1 Solitaire', 'Finish today''s draw-1 Solitaire deal.', 'daily', 'skill', 'puzzle', 'medium', 'daily_puzzle_win', '{"game":"solitaire","difficulty":"draw-1"}'::jsonb, 1, 375, 100, true, 'assignment', NULL),
    ('daily_tetris_score', 'Score 25,000 in Lateris', 'Finish a Lateris run with at least 25,000 points.', 'daily', 'skill', 'arcade', 'medium', 'arcade_score', '{"game":"tetris"}'::jsonb, 25000, 375, 100, true, 'assignment', NULL),
    ('daily_2048_score', 'Score 12,000 in 2048', 'Finish a 2048 run with at least 12,000 points.', 'daily', 'skill', 'arcade', 'medium', 'arcade_score', '{"game":"2048"}'::jsonb, 12000, 375, 100, true, 'assignment', NULL),
    ('daily_snake_score', 'Score 10,000 in Snake', 'Finish a Snake run with at least 10,000 points.', 'daily', 'skill', 'arcade', 'medium', 'arcade_score', '{"game":"snake"}'::jsonb, 10000, 375, 100, true, 'assignment', NULL),
    ('daily_blackjack_hands', 'Play 20 Blackjack hands', 'Finish 20 Blackjack hands.', 'daily', 'skill', 'casino', 'medium', 'room_rounds_played', '{"game":"blackjack"}'::jsonb, 20, 400, 100, true, 'assignment', NULL),
    ('daily_poker_hands', 'Play 10 Poker hands', 'Finish 10 Poker hands with chips committed.', 'daily', 'skill', 'casino', 'medium', 'room_rounds_played', '{"game":"poker"}'::jsonb, 10, 400, 100, true, 'assignment', NULL),
    ('daily_blackjack_wins', 'Win 10 Blackjack hands', 'Win 10 Blackjack hands.', 'daily', 'skill', 'casino', 'medium', 'room_wins', '{"game":"blackjack"}'::jsonb, 10, 400, 100, true, 'assignment', NULL),
    ('daily_poker_wins', 'Win 5 Poker hands', 'Win 5 Poker hands.', 'daily', 'skill', 'casino', 'medium', 'room_wins', '{"game":"poker"}'::jsonb, 5, 400, 100, true, 'assignment', NULL),
    ('daily_chess_games', 'Play a real Chess game', 'Finish 1 Chess game after at least 20 half-moves.', 'daily', 'skill', 'strategy', 'medium', 'room_rounds_played', '{"game":"chess"}'::jsonb, 1, 400, 100, true, 'assignment', NULL),
    ('daily_tron_rounds', 'Play 10 Tron rounds', 'Finish 10 Tron rounds after at least 30 ticks each.', 'daily', 'skill', 'arcade', 'medium', 'room_rounds_played', '{"game":"tron"}'::jsonb, 10, 300, 100, true, 'assignment', NULL),
    ('daily_tron_wins', 'Win 3 Tron rounds', 'Win 3 Tron rounds.', 'daily', 'skill', 'arcade', 'medium', 'room_wins', '{"game":"tron"}'::jsonb, 3, 400, 100, true, 'assignment', NULL),
    ('win_hard_sudoku', 'Win hard Sudoku', 'Solve today''s hard Sudoku.', 'weekly', 'skill', 'puzzle', 'hard', 'daily_puzzle_win', '{"game":"sudoku","difficulty":"hard"}'::jsonb, 1, 750, 100, true, 'assignment', NULL),
    ('solve_hard_nonogram', 'Solve hard Nonogram', 'Solve today''s hard Nonogram.', 'weekly', 'skill', 'puzzle', 'hard', 'daily_puzzle_win', '{"game":"nonogram","difficulty":"hard"}'::jsonb, 1, 750, 100, true, 'assignment', NULL),
    ('clear_hard_minesweeper', 'Clear hard Minesweeper', 'Clear today''s hard Minesweeper board.', 'weekly', 'skill', 'puzzle', 'hard', 'daily_puzzle_win', '{"game":"minesweeper","difficulty":"hard"}'::jsonb, 1, 750, 100, true, 'assignment', NULL),
    ('win_draw_3_solitaire', 'Win draw-3 Solitaire', 'Finish today''s draw-3 Solitaire deal.', 'weekly', 'skill', 'puzzle', 'hard', 'daily_puzzle_win', '{"game":"solitaire","difficulty":"draw-3"}'::jsonb, 1, 750, 100, true, 'assignment', NULL),
    ('weekly_tetris_score', 'Score 100,000 in Lateris', 'Finish a Lateris run with at least 100,000 points.', 'weekly', 'skill', 'arcade', 'hard', 'arcade_score', '{"game":"tetris"}'::jsonb, 100000, 750, 100, true, 'assignment', NULL),
    ('weekly_2048_score', 'Score 40,000 in 2048', 'Finish a 2048 run with at least 40,000 points.', 'weekly', 'skill', 'arcade', 'hard', 'arcade_score', '{"game":"2048"}'::jsonb, 40000, 750, 100, true, 'assignment', NULL),
    ('weekly_snake_score', 'Score 30,000 in Snake', 'Finish a Snake run with at least 30,000 points.', 'weekly', 'skill', 'arcade', 'hard', 'arcade_score', '{"game":"snake"}'::jsonb, 30000, 750, 100, true, 'assignment', NULL),
    ('weekly_blackjack_hands', 'Play 60 Blackjack hands', 'Finish 60 Blackjack hands.', 'weekly', 'casino', 'casino', 'hard', 'room_rounds_played', '{"game":"blackjack"}'::jsonb, 60, 1000, 100, true, 'assignment', NULL),
    ('weekly_poker_hands', 'Play 30 Poker hands', 'Finish 30 Poker hands with chips committed.', 'weekly', 'casino', 'casino', 'hard', 'room_rounds_played', '{"game":"poker"}'::jsonb, 30, 1000, 100, true, 'assignment', NULL),
    ('weekly_blackjack_wins', 'Win 30 Blackjack hands', 'Win 30 Blackjack hands.', 'weekly', 'casino', 'casino', 'hard', 'room_wins', '{"game":"blackjack"}'::jsonb, 30, 1000, 100, true, 'assignment', NULL),
    ('weekly_poker_wins', 'Win 15 Poker hands', 'Win 15 Poker hands.', 'weekly', 'casino', 'casino', 'hard', 'room_wins', '{"game":"poker"}'::jsonb, 15, 1000, 100, true, 'assignment', NULL),
    ('weekly_chess_games', 'Play 5 real Chess games', 'Finish 5 Chess games after at least 20 half-moves each.', 'weekly', 'skill', 'strategy', 'hard', 'room_rounds_played', '{"game":"chess"}'::jsonb, 5, 1500, 100, true, 'assignment', NULL),
    ('weekly_chess_wins', 'Win 2 Chess games', 'Win 2 Chess games.', 'weekly', 'skill', 'strategy', 'hard', 'room_wins', '{"game":"chess"}'::jsonb, 2, 1500, 100, true, 'assignment', NULL),
    ('weekly_tron_rounds', 'Play 40 Tron rounds', 'Finish 40 Tron rounds after at least 30 ticks each.', 'weekly', 'skill', 'arcade', 'hard', 'room_rounds_played', '{"game":"tron"}'::jsonb, 40, 750, 100, true, 'assignment', NULL),
    ('weekly_tron_wins', 'Win 15 Tron rounds', 'Win 15 Tron rounds.', 'weekly', 'skill', 'arcade', 'hard', 'room_wins', '{"game":"tron"}'::jsonb, 15, 900, 100, true, 'assignment', NULL)
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
