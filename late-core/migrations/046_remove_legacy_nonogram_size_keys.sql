DELETE FROM nonogram_games
WHERE size_key IN ('10x10', '15x15', '20x20');

DELETE FROM nonogram_daily_wins
WHERE size_key IN ('10x10', '15x15', '20x20');
