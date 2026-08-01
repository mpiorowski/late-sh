-- All-time daily-win boards used to COUNT(*) over the full history of every
-- *_daily_wins table on each leaderboard refresh, a cost that grows forever
-- with recorded wins. daily_win_totals is the incrementally maintained
-- rollup: every win insert bumps its (game, user_id) row in the same
-- statement (see leaderboard.rs::bump_daily_win_total_sql), so the all-time
-- board query reads one row per player per game regardless of history size.
-- The counter cannot drift: the bump rides the win insert's own statement and
-- fires only when a row was actually inserted, never on same-day replays.
--
-- The backfill below covers wins recorded before this migration. If a
-- pre-rollup binary writes wins between migrate and deploy, rebuild with
-- TRUNCATE daily_win_totals followed by re-running the INSERT ... SELECT.

CREATE TABLE daily_win_totals (
    game VARCHAR NOT NULL,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    wins BIGINT NOT NULL,
    PRIMARY KEY (game, user_id)
);

INSERT INTO daily_win_totals (game, user_id, wins)
SELECT 'sudoku', user_id, COUNT(*) FROM sudoku_daily_wins GROUP BY user_id
UNION ALL
SELECT 'nonogram', user_id, COUNT(*) FROM nonogram_daily_wins GROUP BY user_id
UNION ALL
SELECT 'minesweeper', user_id, COUNT(*) FROM minesweeper_daily_wins GROUP BY user_id
UNION ALL
SELECT 'solitaire', user_id, COUNT(*) FROM solitaire_daily_wins GROUP BY user_id
UNION ALL
SELECT 'le_word', user_id, COUNT(*) FROM le_word_daily_wins GROUP BY user_id
UNION ALL
SELECT 'rubiks_cube', user_id, COUNT(*) FROM rubiks_cube_daily_wins GROUP BY user_id;
