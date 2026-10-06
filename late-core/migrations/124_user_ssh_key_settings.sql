-- Per-key settings blob. Home rail layout is per device: the key a session
-- authenticated with decides whether the room rail and the right sidebar show,
-- so a phone and a desktop on the same linked account stop fighting over one
-- account-level value. Empty object means "inherit the account default".
ALTER TABLE user_ssh_keys
    ADD COLUMN settings JSONB NOT NULL DEFAULT '{}'::jsonb;
