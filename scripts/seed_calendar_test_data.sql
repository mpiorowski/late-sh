\set ON_ERROR_STOP on
BEGIN;
INSERT INTO users(fingerprint,username,settings,is_moderator,is_admin)
SELECT 'seed:calendar:v2:'||account,'cal_'||account,
 jsonb_build_object('clubhouse_tutorial_done',true,'interaction_mode','hybrid','timezone','America/Denver'),account='mod',account='admin'
FROM seed_calendar_keys
ON CONFLICT(fingerprint) DO UPDATE SET is_moderator=EXCLUDED.is_moderator,is_admin=EXCLUDED.is_admin,
 settings=users.settings || jsonb_build_object('clubhouse_tutorial_done',true);
CREATE TEMP TABLE seed_calendar_users ON COMMIT DROP AS
SELECT k.account,u.id FROM seed_calendar_keys k JOIN users u ON u.fingerprint='seed:calendar:v2:'||k.account;
DO $$ BEGIN
 IF EXISTS(SELECT 1 FROM seed_calendar_keys k JOIN user_ssh_keys existing ON existing.fingerprint=k.fingerprint JOIN seed_calendar_users u USING(account) WHERE existing.user_id<>u.id)
 THEN RAISE EXCEPTION 'A fixture key belongs to another account; use a fresh calendar-seed-keys directory'; END IF;
END $$;
INSERT INTO user_ssh_keys(user_id,fingerprint,label)
SELECT u.id,k.fingerprint,'Local calendar fixture' FROM seed_calendar_users u JOIN seed_calendar_keys k USING(account)
ON CONFLICT(fingerprint) DO NOTHING;
INSERT INTO chat_room_members(room_id,user_id,last_read_at)
SELECT r.id,u.id,current_timestamp FROM seed_calendar_users u CROSS JOIN chat_rooms r WHERE r.visibility='public' AND r.auto_join
ON CONFLICT(room_id,user_id) DO NOTHING;
-- Fixture ids, never title/owner matching, define what a rerun may replace.
CREATE TEMP TABLE seed_calendar_ids ON COMMIT DROP AS SELECT n,md5('seed:calendar:v2:event:'||n)::uuid id FROM generate_series(1,16) n;
DELETE FROM calendar_events WHERE id IN(SELECT id FROM seed_calendar_ids);
-- Timed board posts around now: one on, one half an hour out, one tomorrow
-- evening, three overlapping tonight; plus the user's private one.
INSERT INTO calendar_events(id,owner_id,creator_id,title,description,start_at,end_at,creator_timezone,starts_at,ends_at)
SELECT ids.id,
 CASE WHEN n=6 THEN (SELECT id FROM seed_calendar_users WHERE account='user') END,
 (SELECT id FROM seed_calendar_users WHERE account=CASE n WHEN 1 THEN 'mod' WHEN 2 THEN 'other' WHEN 3 THEN 'admin' WHEN 6 THEN 'user' ELSE 'user' END),
 CASE n WHEN 1 THEN 'Movie night · The Matrix' WHEN 2 THEN 'Minecraft raid on the nether' WHEN 3 THEN 'Chess tournament round 1 · 日本語 café' WHEN 4 THEN 'Late-night radio hour' WHEN 5 THEN 'Pool league' WHEN 6 THEN 'Dentist (private)' END,
 'Development fixture. ' || CASE n WHEN 1 THEN 'Started ten minutes ago: the headline already fired, the strip reads on now.' WHEN 2 THEN 'Half an hour out: on the strip and the Live panel.' WHEN 3 THEN 'Tomorrow evening: on the board only until the hour before.' WHEN 6 THEN 'Only cal_user ever sees this one, and only on page 7.' ELSE 'Tonight, overlapping the others.' END,
 current_timestamp + CASE n WHEN 1 THEN interval '-10 minutes' WHEN 2 THEN interval '30 minutes' WHEN 3 THEN interval '26 hours' WHEN 4 THEN interval '3 hours' WHEN 5 THEN interval '3 hours 30 minutes' WHEN 6 THEN interval '5 hours' END,
 current_timestamp + CASE n WHEN 1 THEN interval '110 minutes' WHEN 2 THEN interval '150 minutes' WHEN 3 THEN interval '28 hours' WHEN 4 THEN interval '4 hours' WHEN 5 THEN interval '5 hours' WHEN 6 THEN interval '6 hours' END,
 'America/Denver',
 current_timestamp + CASE n WHEN 1 THEN interval '-10 minutes' WHEN 2 THEN interval '30 minutes' WHEN 3 THEN interval '26 hours' WHEN 4 THEN interval '3 hours' WHEN 5 THEN interval '3 hours 30 minutes' WHEN 6 THEN interval '5 hours' END,
 current_timestamp + CASE n WHEN 1 THEN interval '110 minutes' WHEN 2 THEN interval '150 minutes' WHEN 3 THEN interval '28 hours' WHEN 4 THEN interval '4 hours' WHEN 5 THEN interval '5 hours' WHEN 6 THEN interval '6 hours' END
FROM seed_calendar_ids ids WHERE n<=6;
-- An all-day board post today, which takes the #lounge topic row.
INSERT INTO calendar_events(id,owner_id,creator_id,title,description,start_date,end_date,creator_timezone,starts_at,ends_at)
SELECT ids.id,NULL,(SELECT id FROM seed_calendar_users WHERE account='other'),
 'Hackathon day','Development fixture: all day today, on the #lounge topic row.',
 (current_timestamp AT TIME ZONE 'America/Denver')::date,
 (current_timestamp AT TIME ZONE 'America/Denver')::date+1,
 'America/Denver',
 (((current_timestamp AT TIME ZONE 'America/Denver')::date)::timestamp AT TIME ZONE 'America/Denver'),
 (((current_timestamp AT TIME ZONE 'America/Denver')::date+1)::timestamp AT TIME ZONE 'America/Denver')
FROM seed_calendar_ids ids WHERE n=7;
-- A crowded all-day board day next week, to exercise the compact grid.
INSERT INTO calendar_events(id,owner_id,creator_id,title,description,start_date,end_date,creator_timezone,starts_at,ends_at)
SELECT ids.id,NULL,(SELECT id FROM seed_calendar_users WHERE account='other'),
 'Crowded day post '||n,'Development fixture: a day with many posts.',
 (current_timestamp AT TIME ZONE 'America/Denver')::date+7,
 (current_timestamp AT TIME ZONE 'America/Denver')::date+8,
 'America/Denver',
 (((current_timestamp AT TIME ZONE 'America/Denver')::date+7)::timestamp AT TIME ZONE 'America/Denver'),
 (((current_timestamp AT TIME ZONE 'America/Denver')::date+8)::timestamp AT TIME ZONE 'America/Denver')
FROM seed_calendar_ids ids WHERE n>7;
-- Who is in: the movie night has a crowd, the raid two, the tournament one.
INSERT INTO calendar_rsvps(event_id,user_id)
SELECT ids.id,u.id FROM seed_calendar_ids ids CROSS JOIN seed_calendar_users u
WHERE (ids.n=1) OR (ids.n=2 AND u.account IN('user','other')) OR (ids.n=3 AND u.account='admin')
ON CONFLICT DO NOTHING;
COMMIT;
