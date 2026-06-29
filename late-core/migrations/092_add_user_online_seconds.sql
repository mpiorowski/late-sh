-- Accumulated time a user has spent connected, in seconds. Drives the idle
-- "presence rank" shown beside their name in chat. Accrued per session on
-- disconnect; defaults to 0 for every existing account.
ALTER TABLE users
    ADD COLUMN IF NOT EXISTS online_seconds BIGINT NOT NULL DEFAULT 0;
