-- The Top Drinkers board is retired, and the monthly snapshot no longer
-- grants its DRNK award. Its last month, November 2026, goes to veritas1A4 by
-- hand: the badge shows through December like any single-holder award, and the
-- December rollover announces it if this lands before that roll is claimed.
-- The score is November's buzz as the board summed it, taken when this runs
-- and never recomputed: applied before December it holds the month so far.
--
-- A database without the account (dev, tests) inserts nothing.
INSERT INTO profile_awards (user_id, category, period_month, rank, score_value)
SELECT u.id,
       'top_drinkers',
       DATE '2026-11-01',
       1,
       COALESCE((
           SELECT SUM(p.points)::bigint
           FROM drink_pours p
           WHERE p.user_id = u.id
             AND p.created >= TIMESTAMPTZ '2026-11-01 00:00:00+00'
             AND p.created < TIMESTAMPTZ '2026-12-01 00:00:00+00'
       ), 0)
FROM users u
WHERE LOWER(u.username) = LOWER('veritas1A4')
ON CONFLICT (user_id, category, period_month) DO NOTHING;
