-- Clear pre-hash tokens; stored values are raw tokens and cannot authenticate
-- under the new SHA-256 scheme.
TRUNCATE native_tokens;

ALTER TABLE native_tokens
    ADD COLUMN last_used_at TIMESTAMPTZ,
    ADD COLUMN user_agent   TEXT,
    ADD COLUMN created_ip   TEXT;
