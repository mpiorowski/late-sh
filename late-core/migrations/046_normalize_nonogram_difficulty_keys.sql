DELETE FROM nonogram_games
WHERE size_key NOT IN ('easy', 'medium', 'hard');

DELETE FROM nonogram_daily_wins
WHERE size_key NOT IN ('easy', 'medium', 'hard');

ALTER TABLE nonogram_games
RENAME COLUMN size_key TO difficulty_key;

ALTER TABLE nonogram_daily_wins
RENAME COLUMN size_key TO difficulty_key;
