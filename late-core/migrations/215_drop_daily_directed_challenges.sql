-- Daily challenges are open-only: anyone in the lobby may claim one.
-- Challenges still waiting on a named opponent are withdrawn rather than
-- opened up to everyone, since their poster picked who they wanted to play.
UPDATE daily_matches
SET status = 'cancelled',
    updated = current_timestamp
WHERE status = 'open'
  AND target_user_id IS NOT NULL;

ALTER TABLE daily_matches DROP COLUMN target_user_id;
