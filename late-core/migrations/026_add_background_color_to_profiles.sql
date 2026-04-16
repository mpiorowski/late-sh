-- 026_add_background_color_to_profiles.sql
-- Add black background setting to user profiles

ALTER TABLE profiles ADD COLUMN enable_background_color BOOLEAN NOT NULL DEFAULT FALSE;
