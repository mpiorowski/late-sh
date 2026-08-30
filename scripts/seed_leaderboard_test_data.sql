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
    (1,  'lemoncmd'),
    (2,  'mars'),
    (3,  'astersystem'),
    (4,  'Bambus'),
    (5,  'mojoro'),
    (6,  'cws'),
    (7,  'coko7'),
    (8,  'flagrantior'),
    (9,  'n0ll'),
    (10, 'lunar'),
    (11, 'OvidsMuse'),
    (12, 'beforeuall'),
    (13, 'laksuall'),
    (14, '0x53'),
    (15, 'damax'),
    (16, 'Inkk_is_here'),
    (17, 'andrewg'),
    (18, 'choedev'),
    (19, 'Schnouki'),
    (20, 'fellshard'),
    (21, 'bole'),
    (22, 'odd'),
    (23, 'qmay654'),
    (24, 'mnem'),
    (25, 'lazo'),
    (26, 'imerin'),
    (27, 'mjswenxx'),
    (28, 'crs'),
    (29, 'Shattered'),
    (30, 'HeadedBambus'),
    (31, 'janmar6'),
    (32, 'pixelpirate'),
    (33, 'neonbadger'),
    (34, 'amberbyte'),
    (35, 'crimsonfern'),
    (36, 'velvetlogic'),
    (37, 'quietcomet'),
    (38, 'copperowl'),
    (39, 'syntaxwitch'),
    (40, 'rocketmoss'),
    (41, 'tinysprocket'),
    (42, 'glassharbor'),
    (43, 'midnightcpu'),
    (44, 'arcadewisp'),
    (45, 'luckyvector'),
    (46, 'paperdragon'),
    (47, 'staticriver'),
    (48, 'longname_for_truncation');

-- Usernames are prefixed: users has a UNIQUE index on LOWER(username), so
-- seeding bare handles aborts the whole transaction on any database where a
-- real account already holds one.
INSERT INTO users (fingerprint, username, settings, created, updated, last_seen)
SELECT
    'seed:leaderboard:v2:' || lpad(idx::text, 2, '0'),
    'lb_' || username,
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
SELECT n.idx, u.username, u.id AS user_id
FROM seed_names n
JOIN users u
  ON u.fingerprint = 'seed:leaderboard:v2:' || lpad(n.idx::text, 2, '0');

ALTER TABLE seed_players ADD PRIMARY KEY (idx);

-- Enrich the requested non-system, non-seeded account, or default to the most
-- recently active one so the person viewing the local TUI gets meaningful
-- deep-rank/current-user rows too.
\if :{?leaderboard_username}
CREATE TEMP TABLE seed_current_player ON COMMIT DROP AS
SELECT id AS user_id, username
FROM users
WHERE username NOT IN ('system', 'bot', 'bartender')
  AND fingerprint NOT LIKE 'seed:leaderboard:%'
  AND LOWER(username) = LOWER(:'leaderboard_username')
LIMIT 1;
\else
CREATE TEMP TABLE seed_current_player ON COMMIT DROP AS
SELECT id AS user_id, username
FROM users
WHERE username NOT IN ('system', 'bot', 'bartender')
  AND fingerprint NOT LIKE 'seed:leaderboard:%'
ORDER BY last_seen DESC
LIMIT 1;
\endif

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
DELETE FROM sliding_puzzle_daily_wins w USING seed_players p WHERE w.user_id = p.user_id;
DELETE FROM daily_win_totals t USING seed_players p WHERE t.user_id = p.user_id;
DELETE FROM mud_characters c USING seed_players p WHERE c.user_id = p.user_id;
DELETE FROM door_runs r USING seed_players p WHERE r.user_id = p.user_id;
DELETE FROM door_milestones m USING seed_players p WHERE m.user_id = p.user_id;
DELETE FROM user_online_time_monthly t USING seed_players p WHERE t.user_id = p.user_id;
DELETE FROM user_online_time t USING seed_players p WHERE t.user_id = p.user_id;

-- All-time connected durations, spread from roughly 49 days down to two days.
-- The non-round minute/second offsets exercise the compact duration formatter.
INSERT INTO user_online_time (user_id, total_milliseconds, last_flush_id)
SELECT
    user_id,
    ((50 - idx)::bigint * 86400000) + (idx::bigint * 1234567),
    uuidv7()
FROM seed_players;

-- Current-month connected durations deliberately use a different ordering from
-- all-time, so the paired Late Time windows do not mirror each other.
INSERT INTO user_online_time_monthly
    (month_start, user_id, total_milliseconds, last_flush_id)
SELECT
    date_trunc('month', current_timestamp AT TIME ZONE 'UTC')::date,
    user_id,
    ((49 - ((idx * 17) % 48))::bigint * 3600000) + (idx::bigint * 12345),
    uuidv7()
FROM seed_players;

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

-- Sliding Puzzle is the one daily puzzle whose result metric counts down: the
-- fact is a move count, so the strongest players (lowest idx) get the smallest
-- numbers, and the per-difficulty offsets scale with the 3x3/4x4/5x5 boards.
WITH month_days AS (
    SELECT day::date AS puzzle_date
    FROM generate_series(date_trunc('month', current_date)::date, current_date, interval '1 day') AS day
), difficulties(difficulty_key, day_offset, move_offset) AS (
    VALUES ('easy'::text, 3, 0), ('medium'::text, 7, 140), ('hard'::text, 12, 380)
)
INSERT INTO sliding_puzzle_daily_wins (user_id, difficulty_key, puzzle_date, moves, created, updated)
SELECT
    p.user_id,
    d.difficulty_key,
    m.puzzle_date,
    28 + p.idx * 2 + EXTRACT(day FROM m.puzzle_date)::int + d.move_offset,
    m.puzzle_date::timestamptz + interval '18 hours',
    m.puzzle_date::timestamptz + interval '18 hours'
FROM seed_players p
CROSS JOIN month_days m
CROSS JOIN difficulties d
WHERE EXTRACT(day FROM m.puzzle_date)::int <= 26 - CEIL(p.idx / 2.0)::int - d.day_offset;

-- Lateania characters for the Games boards: paired levels so the experience
-- tiebreak is visible, classes cycling the full roster, and the top half of
-- the field carrying Frontier rooms (2000..=2999, 50 per zone) at spread
-- depths. The blob shape mirrors the game's save schema; unknown fields
-- default on load, and these users never log in anyway.
INSERT INTO mud_characters (user_id, data)
SELECT
    p.user_id,
    jsonb_build_object(
        'version', 17,
        'class', (ARRAY[
            'warrior', 'mage', 'cleric', 'rogue', 'ranger', 'druid',
            'necromancer', 'bard', 'monk', 'paladin', 'warlock', 'berserker',
            'beastlord', 'skald', 'runemaster', 'valewalker', 'spiritmaster'
        ])[1 + (p.idx % 17)],
        'level', GREATEST(3, 50 - ((p.idx - 1) / 2) * 2),
        'xp', GREATEST(3, 50 - ((p.idx - 1) / 2) * 2) * 40000 + (48 - p.idx) * 977,
        'hp', 200,
        'gold', 50 * p.idx,
        'visited',
        CASE
            WHEN p.idx <= 24 THEN jsonb_build_array(1, 5, 12, 2000, 2000 + (24 - p.idx) * 41)
            ELSE jsonb_build_array(1, 5, 12)
        END
    )
FROM seed_players p
ON CONFLICT (user_id, slot) DO UPDATE SET
    data = EXCLUDED.data,
    updated = current_timestamp;

-- DCSS door boards: the first six players are winners (their runs end at the
-- surface, depth 1, the way crawl really stamps a win), even indexes died
-- this month and odd ones long ago, and every seventh player quit (quits stay
-- off the wins board). source_file carries a seed: prefix so the global
-- unique (game, source_file, source_offset) key can never collide with lines
-- the real ingestion pipe lands.
INSERT INTO door_runs (game, user_id, ended_at, result, score, depth, turns, raw, source_file, source_offset)
SELECT
    'dcss',
    p.user_id,
    current_timestamp - CASE WHEN p.idx % 2 = 0 THEN (p.idx % 20) ELSE 40 + p.idx END * interval '1 day',
    CASE
        WHEN p.idx <= 6 THEN 'win'
        WHEN p.idx % 7 = 0 THEN 'quit'
        ELSE 'death'
    END,
    2000000 - p.idx * 37000,
    CASE WHEN p.idx <= 6 THEN 1 ELSE GREATEST(2, 28 - (p.idx % 27)) END,
    30000 + p.idx * 700,
    '{}'::jsonb,
    'seed:logfile',
    p.idx
FROM seed_players p
WHERE p.idx <= 36;

-- The winners' Orb milestones carry the real dive depth, exercising the dive
-- board's milestones-over-runs union (a surface exit alone would rank them
-- at depth 1).
INSERT INTO door_milestones (game, user_id, kind, occurred_at, raw, source_file, source_offset)
SELECT
    'dcss',
    p.user_id,
    'orb',
    current_timestamp - CASE WHEN p.idx % 2 = 0 THEN (p.idx % 20) ELSE 40 + p.idx END * interval '1 day',
    jsonb_build_object('absdepth', (28 - p.idx)::text),
    'seed:milestones',
    p.idx
FROM seed_players p
WHERE p.idx <= 6;

-- NetHack door boards, same shape shifted by four so the two doors' boards
-- don't mirror each other: the first four players ascended (NetHack's maxlvl
-- is already the deepest level reached, so no milestone union is needed —
-- winners carry a real depth on the run row itself).
INSERT INTO door_runs (game, user_id, ended_at, result, score, depth, turns, raw, source_file, source_offset)
SELECT
    'nethack',
    p.user_id,
    current_timestamp - CASE WHEN p.idx % 2 = 1 THEN (p.idx % 18) ELSE 35 + p.idx END * interval '1 day',
    CASE
        WHEN p.idx BETWEEN 5 AND 8 THEN 'win'
        WHEN p.idx % 9 = 0 THEN 'quit'
        ELSE 'death'
    END,
    3200000 - p.idx * 61000,
    CASE WHEN p.idx BETWEEN 5 AND 8 THEN 45 + p.idx ELSE GREATEST(2, 30 - (p.idx % 29)) END,
    45000 + p.idx * 900,
    '{}'::jsonb,
    'seed:xlogfile',
    p.idx
FROM seed_players p
WHERE p.idx <= 32;

-- The ascenders' Amulet pickup milestones (no depth payload; NetHack's dive
-- board reads run maxlvl only).
INSERT INTO door_milestones (game, user_id, kind, occurred_at, raw, source_file, source_offset)
SELECT
    'nethack',
    p.user_id,
    'amulet',
    current_timestamp - CASE WHEN p.idx % 2 = 1 THEN (p.idx % 18) ELSE 35 + p.idx END * interval '1 day',
    '{}'::jsonb,
    'seed:livelog',
    p.idx
FROM seed_players p
WHERE p.idx BETWEEN 5 AND 8;

-- Brogue door boards, shifted again so no two doors mirror: players 9-11
-- escaped, 12-13 mastered (both count on the wins board), every eleventh
-- quit. Brogue has no milestone stream and the run row's deepestLevel is
-- already the run maximum, so no milestone union is needed. The source_file
-- mirrors the real per-player frame id shape.
INSERT INTO door_runs (game, user_id, ended_at, result, score, depth, turns, raw, source_file, source_offset)
SELECT
    'brogue',
    p.user_id,
    current_timestamp - CASE WHEN p.idx % 3 = 0 THEN (p.idx % 15) ELSE 30 + p.idx END * interval '1 day',
    CASE
        WHEN p.idx BETWEEN 9 AND 11 THEN 'win'
        WHEN p.idx BETWEEN 12 AND 13 THEN 'mastery'
        WHEN p.idx % 11 = 0 THEN 'quit'
        ELSE 'death'
    END,
    24000 - p.idx * 430,
    CASE
        WHEN p.idx BETWEEN 9 AND 11 THEN 26
        WHEN p.idx BETWEEN 12 AND 13 THEN 40
        ELSE GREATEST(2, 24 - (p.idx % 23))
    END,
    9000 + p.idx * 300,
    '{}'::jsonb,
    'seed:players/lb_seed_' || p.idx || '/BrogueRunHistory.txt',
    p.idx
FROM seed_players p
WHERE p.idx <= 28;

-- Non-destructive current-player enrichment.
INSERT INTO user_chips (user_id, balance)
SELECT user_id, 7500 FROM seed_current_player
ON CONFLICT (user_id) DO UPDATE SET
    balance = GREATEST(user_chips.balance, EXCLUDED.balance),
    updated = current_timestamp;

INSERT INTO user_online_time (user_id, total_milliseconds, last_flush_id)
SELECT user_id, 9 * 86400000 + 7 * 3600000 + 23 * 60000, uuidv7()
FROM seed_current_player
ON CONFLICT (user_id) DO UPDATE SET
    total_milliseconds = GREATEST(
        user_online_time.total_milliseconds,
        EXCLUDED.total_milliseconds
    );

INSERT INTO user_online_time_monthly
    (month_start, user_id, total_milliseconds, last_flush_id)
SELECT
    date_trunc('month', current_timestamp AT TIME ZONE 'UTC')::date,
    user_id,
    27 * 3600000 + 23 * 60000,
    uuidv7()
FROM seed_current_player
ON CONFLICT (month_start, user_id) DO UPDATE SET
    total_milliseconds = GREATEST(
        user_online_time_monthly.total_milliseconds,
        EXCLUDED.total_milliseconds
    );

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

INSERT INTO sliding_puzzle_daily_wins (user_id, difficulty_key, puzzle_date, moves)
SELECT user_id, 'medium', current_date, 84 FROM seed_current_player
ON CONFLICT (user_id, difficulty_key, puzzle_date) DO NOTHING;

-- A mid-rank Lateania character, only when the player has none: a real save
-- must never be overwritten, so this is insert-or-nothing, not upsert.
INSERT INTO mud_characters (user_id, data)
SELECT
    user_id,
    jsonb_build_object(
        'version', 17,
        'class', 'ranger',
        'level', 29,
        'xp', 29 * 40000,
        'hp', 300,
        'gold', 800,
        'visited', jsonb_build_array(1, 5, 12, 2000, 2400)
    )
FROM seed_current_player
ON CONFLICT (user_id, slot) DO NOTHING;

-- A representative mid-field DCSS death; real ingested runs are untouched
-- (distinct seed: source_file).
INSERT INTO door_runs (game, user_id, ended_at, result, score, depth, turns, raw, source_file, source_offset)
SELECT 'dcss', user_id, current_timestamp - interval '3 days', 'death', 214000, 15, 41000, '{}'::jsonb, 'seed:logfile', 999999
FROM seed_current_player
ON CONFLICT (game, source_file, source_offset) DO NOTHING;

-- And a mid-field NetHack death to match.
INSERT INTO door_runs (game, user_id, ended_at, result, score, depth, turns, raw, source_file, source_offset)
SELECT 'nethack', user_id, current_timestamp - interval '5 days', 'death', 388000, 18, 52000, '{}'::jsonb, 'seed:xlogfile', 999999
FROM seed_current_player
ON CONFLICT (game, source_file, source_offset) DO NOTHING;

-- And a mid-field Brogue death.
INSERT INTO door_runs (game, user_id, ended_at, result, score, depth, turns, raw, source_file, source_offset)
SELECT 'brogue', user_id, current_timestamp - interval '4 days', 'death', 12400, 14, 6200, '{}'::jsonb, 'seed:players/current/BrogueRunHistory.txt', 999999
FROM seed_current_player
ON CONFLICT (game, source_file, source_offset) DO NOTHING;

INSERT INTO le_word_daily_wins (user_id, puzzle_date, score)
SELECT c.user_id, current_date - 400 + g.n * 9, 1 + (g.n % 6)
FROM seed_current_player c
CROSS JOIN generate_series(1, 24) AS g(n)
ON CONFLICT (user_id, puzzle_date) DO NOTHING;

-- The rollup the all-time daily-win boards read. Production maintains it
-- inside each win-insert statement; this seed writes win rows raw, so rebuild
-- the seed players' and enriched current player's totals from the tables just
-- filled.
INSERT INTO daily_win_totals (game, user_id, wins)
SELECT w.game, w.user_id, COUNT(*)
FROM (
    SELECT 'sudoku' AS game, user_id FROM sudoku_daily_wins
    UNION ALL
    SELECT 'nonogram', user_id FROM nonogram_daily_wins
    UNION ALL
    SELECT 'minesweeper', user_id FROM minesweeper_daily_wins
    UNION ALL
    SELECT 'solitaire', user_id FROM solitaire_daily_wins
    UNION ALL
    SELECT 'le_word', user_id FROM le_word_daily_wins
    UNION ALL
    SELECT 'rubiks_cube', user_id FROM rubiks_cube_daily_wins
    UNION ALL
    SELECT 'sliding_puzzle', user_id FROM sliding_puzzle_daily_wins
) w
JOIN (
    SELECT user_id FROM seed_players
    UNION ALL
    SELECT user_id FROM seed_current_player
) p ON p.user_id = w.user_id
GROUP BY w.game, w.user_id
ON CONFLICT (game, user_id) DO UPDATE SET wins = EXCLUDED.wins;

COMMIT;

SELECT 'synthetic users' AS metric, COUNT(*)::bigint AS value
FROM users WHERE fingerprint LIKE 'seed:leaderboard:v2:%'
UNION ALL
SELECT 'seed chip ledger rows', COUNT(*) FROM chip_ledger WHERE source_kind = 'leaderboard_seed'
UNION ALL
SELECT 'seed score events', COUNT(*) FROM game_score_events e JOIN users u ON u.id = e.user_id WHERE u.fingerprint LIKE 'seed:leaderboard:v2:%'
UNION ALL
SELECT 'seed online times', COUNT(*) FROM user_online_time t JOIN users u ON u.id = t.user_id WHERE u.fingerprint LIKE 'seed:leaderboard:v2:%'
UNION ALL
SELECT 'seed monthly times', COUNT(*) FROM user_online_time_monthly t JOIN users u ON u.id = t.user_id WHERE u.fingerprint LIKE 'seed:leaderboard:v2:%'
UNION ALL
SELECT 'seed Le Word wins', COUNT(*) FROM le_word_daily_wins w JOIN users u ON u.id = w.user_id WHERE u.fingerprint LIKE 'seed:leaderboard:v2:%'
UNION ALL
SELECT 'seed other daily wins',
       (SELECT COUNT(*) FROM sudoku_daily_wins w JOIN users u ON u.id = w.user_id WHERE u.fingerprint LIKE 'seed:leaderboard:v2:%')
     + (SELECT COUNT(*) FROM nonogram_daily_wins w JOIN users u ON u.id = w.user_id WHERE u.fingerprint LIKE 'seed:leaderboard:v2:%')
     + (SELECT COUNT(*) FROM solitaire_daily_wins w JOIN users u ON u.id = w.user_id WHERE u.fingerprint LIKE 'seed:leaderboard:v2:%')
     + (SELECT COUNT(*) FROM minesweeper_daily_wins w JOIN users u ON u.id = w.user_id WHERE u.fingerprint LIKE 'seed:leaderboard:v2:%')
     + (SELECT COUNT(*) FROM rubiks_cube_daily_wins w JOIN users u ON u.id = w.user_id WHERE u.fingerprint LIKE 'seed:leaderboard:v2:%')
     + (SELECT COUNT(*) FROM sliding_puzzle_daily_wins w JOIN users u ON u.id = w.user_id WHERE u.fingerprint LIKE 'seed:leaderboard:v2:%');
