-- Populate the local Docker Compose database with synthetic activity that
-- exercises every player-stat leaderboard. The seed owns users whose
-- fingerprints begin with seed:leaderboard:v2: and can be rerun safely.
--
-- Prefer scripts/seed_leaderboard_test_data.sh to invoke this file.

\set ON_ERROR_STOP on

BEGIN;

CREATE TEMP TABLE seed_names (
    idx integer PRIMARY KEY,
    username text NOT NULL UNIQUE
) ON COMMIT DROP;

INSERT INTO seed_names (idx, username) VALUES
    (1,  'lb_lemoncmd'),
    (2,  'lb_mars'),
    (3,  'lb_astersystem'),
    (4,  'lb_Bambus'),
    (5,  'lb_mojoro'),
    (6,  'lb_cws'),
    (7,  'lb_coko7'),
    (8,  'lb_flagrantior'),
    (9,  'lb_n0ll'),
    (10, 'lb_lunar'),
    (11, 'lb_OvidsMuse'),
    (12, 'lb_beforeuall'),
    (13, 'lb_laksuall'),
    (14, 'lb_0x53'),
    (15, 'lb_damax'),
    (16, 'lb_Inkk_is_here'),
    (17, 'lb_andrewg'),
    (18, 'lb_choedev'),
    (19, 'lb_Schnouki'),
    (20, 'lb_fellshard'),
    (21, 'lb_bole'),
    (22, 'lb_odd'),
    (23, 'lb_qmay654'),
    (24, 'lb_mnem'),
    (25, 'lb_lazo'),
    (26, 'lb_imerin'),
    (27, 'lb_mjswenxx'),
    (28, 'lb_crs'),
    (29, 'lb_Shattered'),
    (30, 'lb_HeadedBambus'),
    (31, 'lb_janmar6'),
    (32, 'lb_pixelpirate'),
    (33, 'lb_neonbadger'),
    (34, 'lb_amberbyte'),
    (35, 'lb_crimsonfern'),
    (36, 'lb_velvetlogic'),
    (37, 'lb_quietcomet'),
    (38, 'lb_copperowl'),
    (39, 'lb_syntaxwitch'),
    (40, 'lb_rocketmoss'),
    (41, 'lb_tinysprocket'),
    (42, 'lb_glassharbor'),
    (43, 'lb_midnightcpu'),
    (44, 'lb_arcadewisp'),
    (45, 'lb_luckyvector'),
    (46, 'lb_paperdragon'),
    (47, 'lb_staticriver'),
    (48, 'lb_longname_for_truncation');

INSERT INTO users (fingerprint, username, settings, created, updated, last_seen)
SELECT
    'seed:leaderboard:v2:' || lpad(idx::text, 2, '0'),
    username,
    jsonb_build_object('leaderboard_seed', true, 'seed_version', 2),
    current_timestamp - (idx || ' days')::interval,
    current_timestamp,
    current_timestamp - (idx || ' minutes')::interval
FROM seed_names
ON CONFLICT (fingerprint) DO UPDATE SET
    username = EXCLUDED.username,
    settings = users.settings || EXCLUDED.settings,
    updated = current_timestamp;

CREATE TEMP TABLE seed_players ON COMMIT DROP AS
SELECT n.idx, n.username, u.id AS user_id
FROM seed_names n
JOIN users u
  ON u.fingerprint = 'seed:leaderboard:v2:' || lpad(n.idx::text, 2, '0');

ALTER TABLE seed_players ADD PRIMARY KEY (idx);

-- Enrich the most recently active non-system, non-seeded account so the person
-- viewing the local TUI gets meaningful deep-rank/current-user rows too.
CREATE TEMP TABLE seed_current_player ON COMMIT DROP AS
SELECT id AS user_id, username
FROM users
WHERE username NOT IN ('system', 'bot', 'bartender')
  AND fingerprint NOT LIKE 'seed:leaderboard:%'
ORDER BY last_seen DESC
LIMIT 1;

-- Synthetic users are owned entirely by this seed. Clear their prior generated
-- facts so rerunning the script is deterministic while preserving real users.
DELETE FROM chip_ledger l USING seed_players p WHERE l.user_id = p.user_id;
DELETE FROM game_score_events e USING seed_players p WHERE e.user_id = p.user_id;
DELETE FROM traffic_track_scores s USING seed_players p WHERE s.user_id = p.user_id;
DELETE FROM traffic_high_scores s USING seed_players p WHERE s.user_id = p.user_id;
DELETE FROM tetris_high_scores s USING seed_players p WHERE s.user_id = p.user_id;
DELETE FROM twenty_forty_eight_high_scores s USING seed_players p WHERE s.user_id = p.user_id;
DELETE FROM snake_high_scores s USING seed_players p WHERE s.user_id = p.user_id;
DELETE FROM sudoku_daily_wins w USING seed_players p WHERE w.user_id = p.user_id;
DELETE FROM nonogram_daily_wins w USING seed_players p WHERE w.user_id = p.user_id;
DELETE FROM solitaire_daily_wins w USING seed_players p WHERE w.user_id = p.user_id;
DELETE FROM minesweeper_daily_wins w USING seed_players p WHERE w.user_id = p.user_id;
DELETE FROM rubiks_cube_daily_wins w USING seed_players p WHERE w.user_id = p.user_id;
DELETE FROM le_word_daily_wins w USING seed_players p WHERE w.user_id = p.user_id;

-- Balances and a multi-event monthly chip ledger. Shop spending is present but
-- intentionally excluded by the production monthly-earners query.
INSERT INTO user_chips (user_id, balance, created, updated)
SELECT user_id, 120000 - idx * 1300, current_timestamp - interval '90 days', current_timestamp
FROM seed_players
ON CONFLICT (user_id) DO UPDATE SET
    balance = EXCLUDED.balance,
    updated = current_timestamp;

WITH targets AS (
    SELECT *, 65000 - idx * 900 AS earned
    FROM seed_players
), entries AS (
    SELECT
        t.user_id,
        v.delta,
        v.reason,
        'leaderboard_seed'::text AS source_kind,
        'leaderboard-v2-' || lpad(t.idx::text, 2, '0') || '-' || v.part AS source_ref,
        date_trunc('month', current_timestamp)
            + ((t.idx + v.day_offset) % GREATEST(1, EXTRACT(day FROM current_date)::int)) * interval '1 day'
            + interval '12 hours' AS created_at
    FROM targets t
    CROSS JOIN LATERAL (
        VALUES
            ((t.earned * 6 / 10)::bigint, 'daily_reward'::text, 'daily', 1),
            ((t.earned * 5 / 10)::bigint, 'game_reward'::text,  'game',  5),
            ((-t.earned / 10)::bigint,    'gift'::text,         'gift',  9),
            (-5000::bigint,               'shop_purchase'::text,'shop', 12)
    ) AS v(delta, reason, part, day_offset)
)
INSERT INTO chip_ledger (user_id, delta, reason, source_kind, source_ref, created_at)
SELECT user_id, delta, reason, source_kind, source_ref, created_at
FROM entries;

INSERT INTO chip_ledger (user_id, delta, reason, source_kind, source_ref, created_at)
SELECT
    user_id,
    80000 + idx * 250,
    'historic_reward',
    'leaderboard_seed',
    'leaderboard-v2-' || lpad(idx::text, 2, '0') || '-previous-month',
    date_trunc('month', current_timestamp) - interval '10 days'
FROM seed_players;

-- All-time score tables are deliberately older than this month; monthly boards
-- are driven by the score event ledger below, so the two views differ.
INSERT INTO tetris_high_scores (user_id, score, created, updated)
SELECT user_id, 2500000 - idx * 30000, current_timestamp - interval '180 days', current_timestamp - interval '60 days'
FROM seed_players;

INSERT INTO twenty_forty_eight_high_scores (user_id, score, created, updated)
SELECT user_id, 160000 - idx * 1500, current_timestamp - interval '180 days', current_timestamp - interval '60 days'
FROM seed_players;

INSERT INTO snake_high_scores (user_id, score, created, updated)
SELECT user_id, 85000 - idx * 1200, current_timestamp - interval '180 days', current_timestamp - interval '60 days'
FROM seed_players;

WITH traffic_totals AS (
    SELECT *, 4200 - idx * 50 AS total_score
    FROM seed_players
), tracks(track_key, track_idx) AS (
    VALUES
        ('Batin', 1),
        ('Route 66', 2),
        ('Eurotrip', 3),
        ('The Realm', 4),
        ('Cosmic Highway', 5),
        ('Chaos Highway', 6)
)
INSERT INTO traffic_track_scores (user_id, track_key, score, created, updated)
SELECT
    t.user_id,
    tracks.track_key,
    t.total_score / 6 + CASE WHEN tracks.track_idx <= t.total_score % 6 THEN 1 ELSE 0 END,
    current_timestamp - interval '180 days',
    current_timestamp - interval '60 days'
FROM traffic_totals t
CROSS JOIN tracks;

INSERT INTO traffic_high_scores (user_id, score, created, updated)
SELECT user_id, 4200 - idx * 50, current_timestamp - interval '180 days', current_timestamp - interval '60 days'
FROM seed_players;

WITH monthly_scores AS (
    SELECT
        p.user_id,
        p.idx,
        v.game,
        v.score,
        v.event_no
    FROM seed_players p
    CROSS JOIN LATERAL (
        VALUES
            ('tetris'::text, 2250000 - p.idx * 31000, 1),
            ('tetris'::text, 1900000 - p.idx * 25000, 2),
            ('2048'::text,   135000 - p.idx * 1500, 1),
            ('2048'::text,   110000 - p.idx * 1200, 2),
            ('snake'::text,   70000 - p.idx * 1000, 1),
            ('snake'::text,   56000 - p.idx * 800,  2),
            ('traffic'::text,  3300 - p.idx * 40,   1),
            ('traffic'::text,  2800 - p.idx * 32,   2)
    ) AS v(game, score, event_no)
)
INSERT INTO game_score_events (user_id, game, score, created_at)
SELECT
    user_id,
    game,
    score,
    date_trunc('month', current_timestamp)
        + ((idx + event_no * 3) % GREATEST(1, EXTRACT(day FROM current_date)::int)) * interval '1 day'
        + interval '15 hours'
FROM monthly_scores;

-- Current-month daily-puzzle results. Participation tapers across 48 users and
-- differs per difficulty, creating dense but nonuniform Arcade Wins rankings.
WITH month_days AS (
    SELECT day::date AS puzzle_date
    FROM generate_series(
        date_trunc('month', current_date)::date,
        current_date,
        interval '1 day'
    ) AS day
), difficulties(difficulty_key, day_offset, score_offset) AS (
    VALUES ('easy'::text, 0, 0), ('medium'::text, 3, 700), ('hard'::text, 8, 1400)
)
INSERT INTO sudoku_daily_wins (user_id, difficulty_key, puzzle_date, score, created, updated)
SELECT
    p.user_id,
    d.difficulty_key,
    m.puzzle_date,
    6000 - p.idx * 31 - EXTRACT(day FROM m.puzzle_date)::int * 7 - d.score_offset,
    m.puzzle_date::timestamptz + interval '12 hours',
    m.puzzle_date::timestamptz + interval '12 hours'
FROM seed_players p
CROSS JOIN month_days m
CROSS JOIN difficulties d
WHERE EXTRACT(day FROM m.puzzle_date)::int <= 26 - CEIL(p.idx / 2.0)::int - d.day_offset;

WITH month_days AS (
    SELECT day::date AS puzzle_date
    FROM generate_series(date_trunc('month', current_date)::date, current_date, interval '1 day') AS day
), difficulties(difficulty_key, day_offset) AS (
    VALUES ('easy'::text, 1), ('medium'::text, 5), ('hard'::text, 10)
)
INSERT INTO nonogram_daily_wins (user_id, difficulty_key, puzzle_date, created, updated)
SELECT
    p.user_id,
    d.difficulty_key,
    m.puzzle_date,
    m.puzzle_date::timestamptz + interval '13 hours',
    m.puzzle_date::timestamptz + interval '13 hours'
FROM seed_players p
CROSS JOIN month_days m
CROSS JOIN difficulties d
WHERE EXTRACT(day FROM m.puzzle_date)::int <= 26 - CEIL(p.idx / 2.0)::int - d.day_offset;

WITH month_days AS (
    SELECT day::date AS puzzle_date
    FROM generate_series(date_trunc('month', current_date)::date, current_date, interval '1 day') AS day
), difficulties(difficulty_key, day_offset, score_offset) AS (
    VALUES ('draw-1'::text, 0, 0), ('draw-3'::text, 7, 900)
)
INSERT INTO solitaire_daily_wins (user_id, difficulty_key, puzzle_date, score, created, updated)
SELECT
    p.user_id,
    d.difficulty_key,
    m.puzzle_date,
    9000 - p.idx * 37 - EXTRACT(day FROM m.puzzle_date)::int * 11 - d.score_offset,
    m.puzzle_date::timestamptz + interval '14 hours',
    m.puzzle_date::timestamptz + interval '14 hours'
FROM seed_players p
CROSS JOIN month_days m
CROSS JOIN difficulties d
WHERE EXTRACT(day FROM m.puzzle_date)::int <= 26 - CEIL(p.idx / 2.0)::int - d.day_offset;

WITH month_days AS (
    SELECT day::date AS puzzle_date
    FROM generate_series(date_trunc('month', current_date)::date, current_date, interval '1 day') AS day
), difficulties(difficulty_key, day_offset, score_offset) AS (
    VALUES ('easy'::text, 2, 0), ('medium'::text, 6, 700), ('hard'::text, 11, 1400)
)
INSERT INTO minesweeper_daily_wins (user_id, difficulty_key, puzzle_date, score, created, updated)
SELECT
    p.user_id,
    d.difficulty_key,
    m.puzzle_date,
    7000 - p.idx * 29 - EXTRACT(day FROM m.puzzle_date)::int * 9 - d.score_offset,
    m.puzzle_date::timestamptz + interval '15 hours',
    m.puzzle_date::timestamptz + interval '15 hours'
FROM seed_players p
CROSS JOIN month_days m
CROSS JOIN difficulties d
WHERE EXTRACT(day FROM m.puzzle_date)::int <= 26 - CEIL(p.idx / 2.0)::int - d.day_offset;

WITH month_days AS (
    SELECT day::date AS puzzle_date
    FROM generate_series(date_trunc('month', current_date)::date, current_date, interval '1 day') AS day
)
INSERT INTO rubiks_cube_daily_wins (user_id, puzzle_date, created, updated)
SELECT
    p.user_id,
    m.puzzle_date,
    m.puzzle_date::timestamptz + interval '16 hours',
    m.puzzle_date::timestamptz + interval '16 hours'
FROM seed_players p
CROSS JOIN month_days m
WHERE EXTRACT(day FROM m.puzzle_date)::int <= 26 - CEIL(p.idx / 2.0)::int - 4;

-- Le Word gets three independent shapes: varied current-month totals, long
-- historical streaks, and many older nonconsecutive wins for all-time depth.
WITH month_days AS (
    SELECT day::date AS puzzle_date
    FROM generate_series(date_trunc('month', current_date)::date, current_date, interval '1 day') AS day
)
INSERT INTO le_word_daily_wins (user_id, puzzle_date, score, created, updated)
SELECT
    p.user_id,
    m.puzzle_date,
    1 + ((p.idx + EXTRACT(day FROM m.puzzle_date)::int) % 6),
    m.puzzle_date::timestamptz + interval '17 hours',
    m.puzzle_date::timestamptz + interval '17 hours'
FROM seed_players p
CROSS JOIN month_days m
WHERE EXTRACT(day FROM m.puzzle_date)::int <= GREATEST(1, 26 - ((p.idx * 7) % 24));

INSERT INTO le_word_daily_wins (user_id, puzzle_date, score, created, updated)
SELECT
    p.user_id,
    date_trunc('month', current_date)::date - (10 + p.idx % 4 + g.n),
    1 + ((p.idx + g.n) % 6),
    (date_trunc('month', current_date)::date - (10 + p.idx % 4 + g.n))::timestamptz + interval '17 hours',
    (date_trunc('month', current_date)::date - (10 + p.idx % 4 + g.n))::timestamptz + interval '17 hours'
FROM seed_players p
CROSS JOIN LATERAL generate_series(0, 53 - p.idx) AS g(n)
ON CONFLICT (user_id, puzzle_date) DO NOTHING;

INSERT INTO le_word_daily_wins (user_id, puzzle_date, score, created, updated)
SELECT
    p.user_id,
    current_date - 900 + g.n * 3,
    1 + ((p.idx + g.n) % 6),
    (current_date - 900 + g.n * 3)::timestamptz + interval '17 hours',
    (current_date - 900 + g.n * 3)::timestamptz + interval '17 hours'
FROM seed_players p
CROSS JOIN LATERAL generate_series(1, 20 + (49 - p.idx) * 4) AS g(n)
ON CONFLICT (user_id, puzzle_date) DO NOTHING;

-- Non-destructive current-player enrichment.
INSERT INTO user_chips (user_id, balance)
SELECT user_id, 7500 FROM seed_current_player
ON CONFLICT (user_id) DO UPDATE SET
    balance = GREATEST(user_chips.balance, EXCLUDED.balance),
    updated = current_timestamp;

INSERT INTO chip_ledger (user_id, delta, reason, source_kind, source_ref, created_at)
SELECT user_id, 5075, 'leaderboard_seed', 'leaderboard_seed', 'leaderboard-v2-current-player', current_timestamp
FROM seed_current_player c
WHERE NOT EXISTS (
    SELECT 1 FROM chip_ledger l
    WHERE l.user_id = c.user_id
      AND l.source_ref = 'leaderboard-v2-current-player'
);

INSERT INTO tetris_high_scores (user_id, score, created, updated)
SELECT user_id, 17190, current_timestamp - interval '180 days', current_timestamp - interval '60 days'
FROM seed_current_player
ON CONFLICT (user_id) DO UPDATE SET score = GREATEST(tetris_high_scores.score, EXCLUDED.score);

INSERT INTO twenty_forty_eight_high_scores (user_id, score, created, updated)
SELECT user_id, 123212, current_timestamp - interval '180 days', current_timestamp - interval '60 days'
FROM seed_current_player
ON CONFLICT (user_id) DO UPDATE SET score = GREATEST(twenty_forty_eight_high_scores.score, EXCLUDED.score);

INSERT INTO snake_high_scores (user_id, score, created, updated)
SELECT user_id, 940, current_timestamp - interval '180 days', current_timestamp - interval '60 days'
FROM seed_current_player
ON CONFLICT (user_id) DO UPDATE SET score = GREATEST(snake_high_scores.score, EXCLUDED.score);

WITH tracks(track_key, track_idx) AS (
    VALUES
        ('Batin', 1), ('Route 66', 2), ('Eurotrip', 3),
        ('The Realm', 4), ('Cosmic Highway', 5), ('Chaos Highway', 6)
)
INSERT INTO traffic_track_scores (user_id, track_key, score, created, updated)
SELECT
    c.user_id,
    t.track_key,
    940 / 6 + CASE WHEN t.track_idx <= 940 % 6 THEN 1 ELSE 0 END,
    current_timestamp - interval '180 days',
    current_timestamp - interval '60 days'
FROM seed_current_player c
CROSS JOIN tracks t
ON CONFLICT (user_id, track_key) DO UPDATE SET
    score = GREATEST(traffic_track_scores.score, EXCLUDED.score);

INSERT INTO traffic_high_scores (user_id, score, created, updated)
SELECT user_id, 940, current_timestamp - interval '180 days', current_timestamp - interval '60 days'
FROM seed_current_player
ON CONFLICT (user_id) DO UPDATE SET score = GREATEST(traffic_high_scores.score, EXCLUDED.score);

WITH scores(game, score) AS (
    VALUES ('tetris'::text, 17190), ('2048', 123212), ('snake', 940), ('traffic', 940)
)
INSERT INTO game_score_events (user_id, game, score, created_at)
SELECT c.user_id, s.game, s.score, current_timestamp
FROM seed_current_player c
CROSS JOIN scores s
WHERE NOT EXISTS (
    SELECT 1 FROM game_score_events e
    WHERE e.user_id = c.user_id
      AND e.game = s.game
      AND e.score = s.score
      AND e.created_at >= date_trunc('month', current_timestamp)
);

INSERT INTO sudoku_daily_wins (user_id, difficulty_key, puzzle_date, score)
SELECT user_id, 'easy', current_date, 4200 FROM seed_current_player
ON CONFLICT (user_id, difficulty_key, puzzle_date) DO NOTHING;

INSERT INTO nonogram_daily_wins (user_id, difficulty_key, puzzle_date)
SELECT user_id, 'medium', current_date FROM seed_current_player
ON CONFLICT (user_id, difficulty_key, puzzle_date) DO NOTHING;

INSERT INTO solitaire_daily_wins (user_id, difficulty_key, puzzle_date, score)
SELECT user_id, 'draw-1', current_date, 6200 FROM seed_current_player
ON CONFLICT (user_id, difficulty_key, puzzle_date) DO NOTHING;

INSERT INTO minesweeper_daily_wins (user_id, difficulty_key, puzzle_date, score)
SELECT user_id, 'easy', current_date, 5100 FROM seed_current_player
ON CONFLICT (user_id, difficulty_key, puzzle_date) DO NOTHING;

INSERT INTO rubiks_cube_daily_wins (user_id, puzzle_date)
SELECT user_id, current_date FROM seed_current_player
ON CONFLICT (user_id, puzzle_date) DO NOTHING;

INSERT INTO le_word_daily_wins (user_id, puzzle_date, score)
SELECT c.user_id, current_date - g.n, 1 + (g.n % 6)
FROM seed_current_player c
CROSS JOIN generate_series(0, 6) AS g(n)
ON CONFLICT (user_id, puzzle_date) DO NOTHING;

INSERT INTO le_word_daily_wins (user_id, puzzle_date, score)
SELECT c.user_id, current_date - 400 + g.n * 9, 1 + (g.n % 6)
FROM seed_current_player c
CROSS JOIN generate_series(1, 24) AS g(n)
ON CONFLICT (user_id, puzzle_date) DO NOTHING;

COMMIT;

SELECT 'synthetic users' AS metric, COUNT(*)::bigint AS value
FROM users WHERE fingerprint LIKE 'seed:leaderboard:v2:%'
UNION ALL
SELECT 'seed chip ledger rows', COUNT(*) FROM chip_ledger WHERE source_kind = 'leaderboard_seed'
UNION ALL
SELECT 'seed score events', COUNT(*) FROM game_score_events e JOIN users u ON u.id = e.user_id WHERE u.fingerprint LIKE 'seed:leaderboard:v2:%'
UNION ALL
SELECT 'seed Le Word wins', COUNT(*) FROM le_word_daily_wins w JOIN users u ON u.id = w.user_id WHERE u.fingerprint LIKE 'seed:leaderboard:v2:%'
UNION ALL
SELECT 'seed other daily wins',
       (SELECT COUNT(*) FROM sudoku_daily_wins w JOIN users u ON u.id = w.user_id WHERE u.fingerprint LIKE 'seed:leaderboard:v2:%')
     + (SELECT COUNT(*) FROM nonogram_daily_wins w JOIN users u ON u.id = w.user_id WHERE u.fingerprint LIKE 'seed:leaderboard:v2:%')
     + (SELECT COUNT(*) FROM solitaire_daily_wins w JOIN users u ON u.id = w.user_id WHERE u.fingerprint LIKE 'seed:leaderboard:v2:%')
     + (SELECT COUNT(*) FROM minesweeper_daily_wins w JOIN users u ON u.id = w.user_id WHERE u.fingerprint LIKE 'seed:leaderboard:v2:%')
     + (SELECT COUNT(*) FROM rubiks_cube_daily_wins w JOIN users u ON u.id = w.user_id WHERE u.fingerprint LIKE 'seed:leaderboard:v2:%');
