-- `updated` on the best-score tables is when the best was set: the monthly
-- score boards and the month-end award snapshot window these tables by it.
-- update_score_if_higher used to stamp it on every submit, beaten or not, so
-- whoever played before that fix shipped still carries an older best onto
-- the month's boards and into its awards.
--
-- A score event that reached the stored best before `updated` proves the
-- stamp wrong: bests only grow, and a run raises the best before its event
-- is written, so the fixed submit path can never leave such an event behind.
-- Move `updated` back to the first event that reached the best. A best no
-- event reached (set before score events existed, or by a run still going)
-- has nothing to date it by and stays as it is. Traffic is left alone: its
-- row is the running total of track bests, rewritten on every finish by
-- design.
UPDATE tetris_high_scores best
SET updated = reached.first_at
FROM (
    SELECT h.user_id, MIN(e.created_at) AS first_at
    FROM tetris_high_scores h
    JOIN game_score_events e
      ON e.user_id = h.user_id
     AND e.game = 'tetris'
     AND e.score >= h.score
    GROUP BY h.user_id
) reached
WHERE reached.user_id = best.user_id
  AND reached.first_at < best.updated;

UPDATE twenty_forty_eight_high_scores best
SET updated = reached.first_at
FROM (
    SELECT h.user_id, MIN(e.created_at) AS first_at
    FROM twenty_forty_eight_high_scores h
    JOIN game_score_events e
      ON e.user_id = h.user_id
     AND e.game = '2048'
     AND e.score >= h.score
    GROUP BY h.user_id
) reached
WHERE reached.user_id = best.user_id
  AND reached.first_at < best.updated;

UPDATE snake_high_scores best
SET updated = reached.first_at
FROM (
    SELECT h.user_id, MIN(e.created_at) AS first_at
    FROM snake_high_scores h
    JOIN game_score_events e
      ON e.user_id = h.user_id
     AND e.game = 'snake'
     AND e.score >= h.score
    GROUP BY h.user_id
) reached
WHERE reached.user_id = best.user_id
  AND reached.first_at < best.updated;
