-- Local fixtures only. USERNAME is supplied as a quoted psql variable.
\set ON_ERROR_STOP on
BEGIN;
SELECT pg_advisory_xact_lock(hashtext('seed:irc-history:v1'));

CREATE TEMP TABLE history_viewer ON COMMIT DROP AS
SELECT id, username FROM users WHERE lower(username) = lower(:'history_username');
DO $$ BEGIN
    IF NOT EXISTS (SELECT 1 FROM history_viewer) THEN
        RAISE EXCEPTION 'Local username not found. Connect to the app first, then pass your TUI username.';
    END IF;
    IF EXISTS (SELECT 1 FROM history_viewer WHERE username IN ('system', 'bot', 'bartender', 'history_ada', 'history.lin')) THEN
        RAISE EXCEPTION 'Choose your own local account, rather than a system or fixture account.';
    END IF;
END $$;

INSERT INTO users (fingerprint, username, settings)
VALUES ('seed:irc-history:v1:ada', 'history_ada', '{"clubhouse_tutorial_done":true}'),
       ('seed:irc-history:v1:lin', 'history.lin', '{"clubhouse_tutorial_done":true}')
ON CONFLICT (fingerprint) DO NOTHING;

CREATE TEMP TABLE history_peers ON COMMIT DROP AS
SELECT id, username FROM users WHERE fingerprint IN ('seed:irc-history:v1:ada', 'seed:irc-history:v1:lin');

-- Fixed IDs distinguish our rooms from a user-created room with the same slug.
INSERT INTO chat_rooms (id, kind, visibility, auto_join, slug, topic, created_by)
SELECT md5('seed:irc-history:v1:room:' || visibility)::uuid,
       'topic', visibility, false, 'history-lab',
       'Synthetic IRC history: ' || visibility || ' conversation; scroll up for older pages.',
       (SELECT id FROM history_viewer)
FROM (VALUES ('public'), ('private')) AS rooms(visibility)
ON CONFLICT (id) DO NOTHING;

INSERT INTO chat_rooms (kind, visibility, auto_join, dm_user_a, dm_user_b)
SELECT 'dm', 'dm', false,
       least(v.id::text, p.id::text)::uuid, greatest(v.id::text, p.id::text)::uuid
FROM history_viewer v CROSS JOIN history_peers p
ON CONFLICT (dm_user_a, dm_user_b) WHERE kind = 'dm' DO NOTHING;

CREATE TEMP TABLE history_rooms ON COMMIT DROP AS
SELECT id, visibility AS label, CASE visibility WHEN 'public' THEN 400 ELSE 80 END AS count
FROM chat_rooms WHERE id IN (md5('seed:irc-history:v1:room:public')::uuid, md5('seed:irc-history:v1:room:private')::uuid)
UNION ALL
SELECT r.id, 'dm:' || p.username, 80
FROM chat_rooms r CROSS JOIN history_viewer v JOIN history_peers p ON true
WHERE r.kind = 'dm' AND r.dm_user_a = least(v.id::text, p.id::text)::uuid
  AND r.dm_user_b = greatest(v.id::text, p.id::text)::uuid
UNION ALL
SELECT id, 'lounge', 40 FROM chat_rooms WHERE kind = 'lounge' AND slug = 'lounge';

INSERT INTO chat_room_members (room_id, user_id)
SELECT r.id, u.id FROM history_rooms r
CROSS JOIN (SELECT id FROM history_viewer UNION ALL SELECT id FROM history_peers) u
WHERE r.label NOT LIKE 'dm:%'
   OR u.id IN (SELECT dm_user_a FROM chat_rooms WHERE id = r.id)
   OR u.id IN (SELECT dm_user_b FROM chat_rooms WHERE id = r.id)
ON CONFLICT (room_id, user_id) DO NOTHING;

CREATE TEMP TABLE history_messages ON COMMIT DROP AS
SELECT md5('seed:irc-history:v1:message:' || r.id || ':' || v.id || ':' || n)::uuid AS id,
       r.id AS room_id, r.label, n,
       CASE WHEN n % 4 = 0 THEN v.id
            WHEN r.label LIKE 'dm:%' THEN (SELECT id FROM history_peers WHERE 'dm:' || username = r.label)
            ELSE (SELECT id FROM history_peers ORDER BY username LIMIT 1 OFFSET (n % 2)) END AS user_id,
       date_trunc('minute', current_timestamp) - interval '5 minutes'
           - ((r.count - n) / 2) * interval '6 minutes' AS created,
       '[history fixture ' || r.label || ' ' || lpad(n::text, 3, '0') || '/' || r.count || '] ' ||
       CASE
           WHEN n = r.count - 5 THEN 'Unicode: café, zażółć gęślą jaźń, 日本語, 🌙 ✨'
           WHEN n = r.count - 4 THEN 'Long message: ' || repeat('Moonlight 🌙 over the clubhouse. ', 35)
           WHEN n = r.count - 3 THEN E'First line of one stored message.\nSecond line: both lines should replay together.'
           WHEN n = r.count - 2 THEN 'Edited sample: this is the latest stored body.'
           WHEN n = r.count - 1 THEN 'Reply to the previous numbered message.'
           WHEN n = r.count THEN 'End of seeded history. Older messages are available by scrolling upward.'
           ELSE (ARRAY['Anyone working on a weekend project?', 'Testing reconnect and history pagination.',
                       'The radio is sounding good tonight.', 'Same timestamp as a neighbor; message IDs keep the order stable.'])[1 + n % 4]
       END AS body
FROM history_rooms r CROSS JOIN history_viewer v
CROSS JOIN LATERAL generate_series(1, r.count) AS n;

-- Upsert IDs reserved for these fixtures, preserving every other message.
INSERT INTO chat_messages (id, room_id, user_id, body, created, updated)
SELECT id, room_id, user_id, body, created, created FROM history_messages
ON CONFLICT (id) DO UPDATE SET body = EXCLUDED.body, created = EXCLUDED.created,
    updated = EXCLUDED.updated, reply_to_message_id = NULL, reply_to_user_id = NULL;

UPDATE chat_messages m SET reply_to_message_id = previous.id, reply_to_user_id = previous.user_id
FROM history_messages spec JOIN history_messages previous
  ON previous.room_id = spec.room_id AND previous.n = spec.n - 1
WHERE m.id = spec.id AND spec.body LIKE '%Reply to the previous numbered message.';

SELECT label AS conversation, count(*) AS messages, min(created) AS oldest, max(created) AS newest
FROM history_messages GROUP BY label ORDER BY label;
COMMIT;
