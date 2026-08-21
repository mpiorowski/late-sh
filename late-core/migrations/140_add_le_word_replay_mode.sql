ALTER TABLE le_word_games
    ADD COLUMN mode TEXT NOT NULL DEFAULT 'daily';

ALTER TABLE le_word_games
    ALTER COLUMN puzzle_date DROP NOT NULL;

DELETE FROM le_word_games game
USING (
    SELECT id,
           row_number() OVER (
               PARTITION BY user_id
               ORDER BY puzzle_date DESC, updated DESC, created DESC
           ) AS position
    FROM le_word_games
) ranked
WHERE game.id = ranked.id
  AND ranked.position > 1;

-- Keep the legacy (user_id, puzzle_date) unique constraint so an old pod can
-- finish in-flight daily saves during a rolling deploy. NULL replay dates do
-- not collide under PostgreSQL uniqueness semantics.

ALTER TABLE le_word_games
    ADD CONSTRAINT le_word_games_mode_check
        CHECK (mode IN ('daily', 'replay')),
    ADD CONSTRAINT le_word_games_mode_date_check
        CHECK (
            (mode = 'daily' AND puzzle_date IS NOT NULL)
            OR (mode = 'replay' AND puzzle_date IS NULL)
        ),
    ADD CONSTRAINT le_word_games_user_mode_key UNIQUE (user_id, mode);
