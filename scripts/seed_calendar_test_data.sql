\set ON_ERROR_STOP on
BEGIN;
INSERT INTO users(fingerprint,username,settings,is_moderator,is_admin)
SELECT 'seed:calendar:v1:'||account,'cal_'||account,
 jsonb_build_object('clubhouse_tutorial_done',true,'interaction_mode','hybrid','timezone','America/Denver'),account='mod',account='admin'
FROM seed_calendar_keys
ON CONFLICT(fingerprint) DO UPDATE SET is_moderator=EXCLUDED.is_moderator,is_admin=EXCLUDED.is_admin,
 settings=users.settings || jsonb_build_object('clubhouse_tutorial_done',true);
CREATE TEMP TABLE seed_calendar_users ON COMMIT DROP AS
SELECT k.account,u.id FROM seed_calendar_keys k JOIN users u ON u.fingerprint='seed:calendar:v1:'||k.account;
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
INSERT INTO calendar_preferences(user_id,public)
SELECT id,account='public' FROM seed_calendar_users
ON CONFLICT(user_id) DO UPDATE SET public=EXCLUDED.public,revision=calendar_preferences.revision+1;
-- Fixture IDs, never title/owner matching, define what a rerun may replace.
CREATE TEMP TABLE seed_calendar_ids ON COMMIT DROP AS SELECT n,md5('seed:calendar:v1:event:'||n)::uuid id FROM generate_series(1,24) n;
DELETE FROM calendar_events WHERE id IN(SELECT id FROM seed_calendar_ids);
INSERT INTO calendar_events(id,owner_id,creator_id,creation_tier,mod_editable,title,description,start_date,end_date,creator_timezone,notice_lead_seconds,notice_start,notice_end)
SELECT ids.id,CASE WHEN n IN(2,3,4) THEN (SELECT id FROM seed_calendar_users WHERE account=CASE n WHEN 2 THEN 'user' WHEN 3 THEN 'public' ELSE 'private' END) END,
 (SELECT id FROM seed_calendar_users WHERE account=CASE WHEN n=5 THEN 'mod' ELSE 'admin' END),
 CASE WHEN n=5 THEN 'moderator' ELSE 'admin' END,n=6,
 CASE n WHEN 1 THEN 'Community weekend · protected' WHEN 2 THEN 'My overnight trip' WHEN 3 THEN 'Public calendar · 日本語 café 🌟' WHEN 4 THEN 'Private appointment' WHEN 5 THEN 'Moderator office hours' WHEN 6 THEN 'Delegated meetup' ELSE 'Crowded day event '||n END,
 'Development calendar fixture. Long descriptions and Unicode titles exercise clipping and scrolling.',
 (current_timestamp AT TIME ZONE 'America/Denver')::date,
 (current_timestamp AT TIME ZONE 'America/Denver')::date+CASE WHEN n=2 THEN 3 ELSE 1 END,
 'America/Denver',CASE WHEN n IN(1,2,3,6) THEN 86400 END,
 ((current_timestamp AT TIME ZONE 'America/Denver')::date)::timestamp AT TIME ZONE 'America/Denver' - CASE WHEN n IN(1,2,3,6) THEN interval '24 hours' ELSE interval '0' END,
 (((current_timestamp AT TIME ZONE 'America/Denver')::date+CASE WHEN n=2 THEN 3 ELSE 1 END)::timestamp AT TIME ZONE 'America/Denver')
FROM seed_calendar_ids ids WHERE n<=16;
INSERT INTO calendar_events(id,owner_id,creator_id,creation_tier,title,description,start_at,end_at,creator_timezone,notice_lead_seconds,notice_start,notice_end)
SELECT ids.id,NULL,(SELECT id FROM seed_calendar_users WHERE account='admin'),'admin',
 CASE WHEN n=24 THEN 'Spans midnight → next day' ELSE 'Overlap lane '||n||' · café 日本語' END,'Timed fixture with overlapping lanes.',
 (date_trunc('day',current_timestamp AT TIME ZONE 'America/Denver') AT TIME ZONE 'America/Denver')+interval '9 hours'+((n-17)%3)*interval '30 minutes',
 (date_trunc('day',current_timestamp AT TIME ZONE 'America/Denver') AT TIME ZONE 'America/Denver')+CASE WHEN n=24 THEN interval '30 hours' ELSE interval '13 hours' END,
 'UTC',86400,(date_trunc('day',current_timestamp AT TIME ZONE 'America/Denver') AT TIME ZONE 'America/Denver')+interval '9 hours'+((n-17)%3)*interval '30 minutes'-interval '24 hours',
 (date_trunc('day',current_timestamp AT TIME ZONE 'America/Denver') AT TIME ZONE 'America/Denver')+CASE WHEN n=24 THEN interval '30 hours' ELSE interval '13 hours' END
FROM seed_calendar_ids ids WHERE n>16;
COMMIT;
