ALTER TABLE moderation_audit_log
    DROP CONSTRAINT IF EXISTS moderation_audit_log_actor_user_id_fkey;

ALTER TABLE room_bans
    DROP CONSTRAINT IF EXISTS room_bans_actor_user_id_fkey;

ALTER TABLE server_bans
    DROP CONSTRAINT IF EXISTS server_bans_actor_user_id_fkey;

ALTER TABLE server_bans
    DROP CONSTRAINT IF EXISTS server_bans_target_user_id_fkey;

ALTER TABLE artboard_bans
    DROP CONSTRAINT IF EXISTS artboard_bans_actor_user_id_fkey;
