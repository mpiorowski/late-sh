-- Local development only. Invoked by seed_artboard_test_data.sh, which supplies
-- seed_art_keys and validates splash_piece. A rerun resets only these fixtures.
\set ON_ERROR_STOP on

BEGIN;

CREATE TEMP TABLE seed_art_accounts (
    account text PRIMARY KEY,
    is_moderator boolean NOT NULL DEFAULT false,
    is_admin boolean NOT NULL DEFAULT false
) ON COMMIT DROP;
INSERT INTO seed_art_accounts (account, is_moderator, is_admin) VALUES
    ('artist1', false, false), ('artist2', false, false), ('artist3', false, false),
    ('voter1', false, false), ('voter2', false, false),
    ('voter3', false, false), ('voter4', false, false),
    ('mod1', true, false), ('mod2', true, false),
    ('admin1', false, true), ('admin2', false, true);

INSERT INTO users (fingerprint, username, settings, is_moderator, is_admin)
SELECT 'seed:artboard:v1:' || account, 'art_' || account,
       jsonb_build_object('artboard_seed', true, 'interaction_mode', 'keyboard',
                          'clubhouse_tutorial_done', true),
       is_moderator, is_admin
FROM seed_art_accounts
ON CONFLICT (fingerprint) DO UPDATE SET
    username = EXCLUDED.username,
    settings = users.settings || jsonb_build_object('clubhouse_tutorial_done', true),
    is_moderator = EXCLUDED.is_moderator,
    is_admin = EXCLUDED.is_admin,
    updated = current_timestamp;

CREATE TEMP TABLE seed_art_users ON COMMIT DROP AS
SELECT a.account, u.id AS user_id, u.username
FROM seed_art_accounts a
JOIN users u ON u.fingerprint = 'seed:artboard:v1:' || a.account;
ALTER TABLE seed_art_users ADD PRIMARY KEY (account);

-- A key which was subsequently linked to a real account must not be stolen
-- back. Fail before any fixture changes are committed in that case.
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM seed_art_keys k
        JOIN user_ssh_keys existing ON existing.fingerprint = k.fingerprint
        JOIN seed_art_users u ON u.account = k.account
        WHERE existing.user_id <> u.user_id
    ) THEN
        RAISE EXCEPTION 'A fixture SSH key is linked to another account. Use a fresh tmp/artboard-seed-keys directory.';
    END IF;
END $$;

INSERT INTO user_ssh_keys (user_id, fingerprint, label)
SELECT u.user_id, k.fingerprint, 'Local art fixture'
FROM seed_art_keys k JOIN seed_art_users u USING (account)
ON CONFLICT (fingerprint) DO NOTHING;

-- Directly inserted accounts skip signup's auto-join step. Match its public
-- room memberships so fixtures can reach #lounge and the /mod console.
INSERT INTO chat_room_members (room_id, user_id, last_read_at)
SELECT r.id, u.user_id, current_timestamp
FROM seed_art_users u
CROSS JOIN chat_rooms r
WHERE r.visibility = 'public' AND r.auto_join = true
  AND NOT EXISTS (
      SELECT 1 FROM room_bans b
      WHERE b.room_id = r.id AND b.target_user_id = u.user_id
        AND (b.expires_at IS NULL OR b.expires_at > current_timestamp)
  )
ON CONFLICT (room_id, user_id) DO NOTHING;

CREATE TEMP TABLE seed_art_specs (
    idx integer PRIMARY KEY,
    id uuid NOT NULL UNIQUE,
    title text NOT NULL,
    artist text NOT NULL,
    created timestamptz NOT NULL,
    applause integer NOT NULL,
    owner_marked_nsfw boolean NOT NULL
) ON COMMIT DROP;
INSERT INTO seed_art_specs
SELECT idx, md5('seed:artboard:v1:piece:' || idx)::uuid,
       lpad(idx::text, 2, '0') || ' ' || motif,
       'artist' || (1 + (idx - 1) % 3),
       CASE WHEN idx = 12 THEN current_timestamp
            ELSE ((current_timestamp AT TIME ZONE 'UTC')::date
                  - CASE WHEN idx <= 3 THEN 2 ELSE 1 END) AT TIME ZONE 'UTC'
                 + (10 + idx)::integer * interval '1 minute' END,
       CASE WHEN idx <= 3 THEN 7 - idx ELSE idx % 3 END,
       idx IN (5, 6)
FROM (VALUES
    (1, 'Checkerboard'), (2, 'Diagonal'), (3, 'Rings'), (4, 'Bars'),
    (5, 'Steps'), (6, 'Diamond'), (7, 'Target'), (8, 'Waves'),
    (9, 'X Cross'), (10, 'Grid'), (11, 'Rocket'), (12, 'Fresh Ladder')
) AS motifs(idx, motif);

-- Serialize exactly the Canvas / ArtboardProvenance wire formats. Numbered
-- labels and distinct patterns make every piece recognizable in the splash.
CREATE TEMP TABLE seed_art_glyphs ON COMMIT DROP AS
SELECT idx, x, y, ch
FROM (
    SELECT s.idx, x, y,
           CASE
               WHEN y IN (0, 11) THEN CASE WHEN x IN (0, 31) THEN '+' ELSE '-' END
               WHEN x IN (0, 31) THEN '|'
               WHEN y = 1 THEN substring(rpad('ART TEST ' || s.title, 30, ' ') FROM x FOR 1)
               WHEN y IN (2, 10) THEN ' '
               WHEN CASE s.idx
                   WHEN 1 THEN (x / 2 + y / 2) % 2 = 0
                   WHEN 2 THEN (x - 2 * y + 24) % 6 = 0
                   WHEN 3 THEN (abs(x - 15) / 3 + abs(y - 6)) % 3 = 0
                   WHEN 4 THEN x % 4 IN (0, 1)
                   WHEN 5 THEN x <= y * 3
                   WHEN 6 THEN abs(x - 15) + 2 * abs(y - 6) <= 9
                   WHEN 7 THEN greatest(abs(x - 15) / 2, abs(y - 6)) % 2 = 0
                   WHEN 8 THEN (x + y * y) % 9 < 2
                   WHEN 9 THEN abs(x - 15) = 2 * abs(y - 6)
                   WHEN 10 THEN x % 5 = 0 OR y % 3 = 0
                   WHEN 11 THEN abs(x - 15) <= y - 3 AND abs(x - 15) <= 4
                   WHEN 12 THEN x IN (11, 20) OR (x BETWEEN 11 AND 20 AND y % 2 = 0)
                   END THEN (ARRAY['#', '/', 'o', '|', '=', '*', '@', '~', 'X', '+', '^', 'H'])[s.idx]
               ELSE ' '
           END AS ch
    FROM seed_art_specs s
    CROSS JOIN generate_series(0, 31) AS x
    CROSS JOIN generate_series(0, 11) AS y
) AS grid
WHERE ch <> ' ';

-- Serialize seeding with the app's daily claim; avoid a partially reset queue.
LOCK TABLE artboard_pieces IN SHARE ROW EXCLUSIVE MODE;
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM seed_art_specs s
        JOIN artboard_pieces p ON p.id = s.id
        JOIN users u ON u.id = p.user_id
        WHERE u.fingerprint <> 'seed:artboard:v1:' || s.artist
    ) THEN
        RAISE EXCEPTION 'A fixture piece ID belongs to another account; nothing was seeded.';
    END IF;
END $$;

INSERT INTO artboard_pieces
    (id, created, user_id, title, width, height, canvas, provenance, glyph_count,
     own_share_percent, content_hash, period_month, owner_marked_nsfw)
SELECT s.id, s.created, u.user_id, s.title, 32, 12,
       jsonb_build_object('width', 32, 'height', 12, 'colors', '[]'::jsonb,
           'cells', jsonb_agg(jsonb_build_array(jsonb_build_object('x', g.x, 'y', g.y),
                             jsonb_build_object('Narrow', g.ch)) ORDER BY g.y, g.x)),
       jsonb_build_object('cells', jsonb_agg(jsonb_build_array(
           jsonb_build_object('x', g.x, 'y', g.y), u.username) ORDER BY g.y, g.x)),
       count(*)::integer, 100,
       encode(sha256(convert_to(E'32x12\n' || string_agg(
           g.x || ',' || g.y || ',' || ascii(g.ch) || E'\n', '' ORDER BY g.x, g.y), 'UTF8')), 'hex'),
       date_trunc('month', s.created AT TIME ZONE 'UTC')::date, s.owner_marked_nsfw
FROM seed_art_specs s
JOIN seed_art_users u ON u.account = s.artist
JOIN seed_art_glyphs g USING (idx)
GROUP BY s.idx, s.id, s.created, u.user_id, u.username, s.title, s.owner_marked_nsfw
ON CONFLICT (id) DO UPDATE SET
    created = EXCLUDED.created, updated = current_timestamp,
    title = EXCLUDED.title, width = EXCLUDED.width, height = EXCLUDED.height,
    canvas = EXCLUDED.canvas, provenance = EXCLUDED.provenance,
    glyph_count = EXCLUDED.glyph_count, own_share_percent = EXCLUDED.own_share_percent,
    content_hash = EXCLUDED.content_hash, period_month = EXCLUDED.period_month,
    owner_marked_nsfw = EXCLUDED.owner_marked_nsfw,
    splash_on = NULL, featured_on = NULL, removed_at = NULL;

-- Only fixture pieces are reset, so real accounts' other votes and works stay.
DELETE FROM artboard_piece_votes v USING seed_art_specs s WHERE v.piece_id = s.id;
DELETE FROM artboard_piece_content_votes v USING seed_art_specs s WHERE v.piece_id = s.id;
DELETE FROM artboard_piece_staff_marks m USING seed_art_specs s WHERE m.piece_id = s.id;

INSERT INTO artboard_piece_votes (piece_id, user_id, author_user_id)
SELECT s.id, voter.user_id, artist.user_id
FROM seed_art_specs s
JOIN seed_art_users artist ON artist.account = s.artist
JOIN (VALUES ('voter1', 1), ('voter2', 2), ('voter3', 3), ('voter4', 4),
             ('mod1', 5), ('mod2', 6)) AS hands(account, idx) ON hands.idx <= s.applause
JOIN seed_art_users voter ON voter.account = hands.account;

-- 02: single NSFW vote; 03: NSFW majority; 04: community tie -> SFW.
-- 05: owner override despite SFW votes; 06: mod SFW overrides owner.
-- 07: mod tie -> NSFW; 08: admin SFW overrides mod; 09: admin conflict -> NSFW.
-- 10: SFW votes; 01, 11, 12: clean slates for manual voting.
INSERT INTO artboard_piece_content_votes (piece_id, user_id, author_user_id, nsfw)
SELECT s.id, voter.user_id, artist.user_id, votes.nsfw
FROM (VALUES
    (2, 'voter1', true),
    (3, 'voter1', true), (3, 'voter2', true), (3, 'voter3', false),
    (4, 'voter1', true), (4, 'voter2', true), (4, 'voter3', false), (4, 'voter4', false),
    (5, 'voter1', false), (5, 'voter2', false), (5, 'voter3', false),
    (6, 'voter1', true), (6, 'voter2', true),
    (7, 'voter1', false), (7, 'voter2', false),
    (8, 'voter1', true), (8, 'voter2', true),
    (9, 'voter1', false), (9, 'voter2', false),
    (10, 'voter1', false), (10, 'voter2', false), (10, 'voter3', false)
) AS votes(idx, account, nsfw)
JOIN seed_art_specs s USING (idx)
JOIN seed_art_users artist ON artist.account = s.artist
JOIN seed_art_users voter ON voter.account = votes.account;

INSERT INTO artboard_piece_staff_marks (piece_id, actor_user_id, authority, nsfw, reason)
SELECT s.id, u.user_id, marks.authority, marks.nsfw, 'Synthetic art rating test'
FROM (VALUES
    (6, 'mod1', 'moderator', false),
    (7, 'mod1', 'moderator', false), (7, 'mod2', 'moderator', true),
    (8, 'mod1', 'moderator', true), (8, 'admin1', 'admin', false),
    (9, 'admin1', 'admin', false), (9, 'admin2', 'admin', true)
) AS marks(idx, account, authority, nsfw)
JOIN seed_art_specs s USING (idx)
JOIN seed_art_users u ON u.account = marks.account;

-- Fixtures can be selected repeatedly for mode testing. A day owned by any
-- non-fixture row (including one taken down) is preserved, as in production.
UPDATE artboard_pieces p
SET splash_on = (current_timestamp AT TIME ZONE 'UTC')::date
FROM seed_art_specs s
WHERE p.id = s.id AND s.idx = :'splash_piece'::integer
  AND NOT EXISTS (
      SELECT 1 FROM artboard_pieces
      WHERE splash_on = (current_timestamp AT TIME ZONE 'UTC')::date
  );

SELECT s.idx AS piece, left(p.id::text, 8) AS mod_id, p.title, u.username AS artist,
       s.applause, r.sfw_votes AS sfw, r.nsfw_votes AS nsfw,
       r.owner_marked_nsfw AS owner_flag,
       r.mod_sfw || '/' || r.mod_nsfw AS mod_sfw_nsfw,
       r.admin_sfw || '/' || r.admin_nsfw AS admin_sfw_nsfw,
       p.splash_on,
       p.created < ((current_timestamp AT TIME ZONE 'UTC')::date AT TIME ZONE 'UTC')
           AND p.splash_on IS NULL AS queued
FROM seed_art_specs s
JOIN artboard_pieces p ON p.id = s.id
JOIN users u ON u.id = p.user_id
JOIN artboard_piece_content_ratings r ON r.piece_id = p.id
ORDER BY s.idx;

SELECT left(id::text, 8) AS mod_id, title AS todays_splash
FROM artboard_pieces
WHERE splash_on = (current_timestamp AT TIME ZONE 'UTC')::date;

COMMIT;
