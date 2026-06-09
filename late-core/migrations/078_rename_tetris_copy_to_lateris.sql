UPDATE reward_templates
SET
    title = 'Score 25,000 in Lateris',
    description = 'Finish a Lateris run with at least 25,000 points.',
    updated = current_timestamp
WHERE key = 'daily_tetris_score';

UPDATE reward_templates
SET
    title = 'Score 100,000 in Lateris',
    description = 'Finish a Lateris run with at least 100,000 points.',
    updated = current_timestamp
WHERE key = 'weekly_tetris_score';
