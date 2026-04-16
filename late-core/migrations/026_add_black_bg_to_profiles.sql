-- 026_add_black_bg_to_profiles.sql
-- Add black background setting to user profiles

ALTER TABLE profiles ADD COLUMN enable_black_bg BOOLEAN NOT NULL DEFAULT FALSE;
