-- Multiple Lateania characters per account (up to CHARACTER_SLOTS, see
-- svc.rs). `slot` picks which save a session is playing; every existing
-- character keeps its data by landing in slot 0, unchanged. The old
-- one-row-per-user uniqueness becomes one-row-per-(user, slot).
ALTER TABLE mud_characters
    ADD COLUMN slot SMALLINT NOT NULL DEFAULT 0;

ALTER TABLE mud_characters
    DROP CONSTRAINT mud_characters_user_id_key;

ALTER TABLE mud_characters
    ADD CONSTRAINT mud_characters_user_id_slot_key UNIQUE (user_id, slot);
