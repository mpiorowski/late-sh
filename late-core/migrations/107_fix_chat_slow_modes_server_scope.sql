-- Reconcile chat_slow_modes with the intended schema for server-scoped slow mode.
--
-- Migration 095 was edited in place after it had already been applied in prod
-- (applied 2026-07-05, edited 2026-07-07 in commit 23b974b6). The runner keys
-- _migrations by name, so the rewrite never re-ran: prod kept room_id NOT NULL
-- plus a plain UNIQUE (room_id, target_user_id) constraint. That made
-- `/mod slow server @name ...` fail (ChatSlowMode::activate_server inserts a NULL
-- room_id, violating NOT NULL and missing a matching ON CONFLICT arbiter).
--
-- This migration is idempotent: on a fresh DB that already ran the current 095 it
-- is a no-op; on prod it applies the nullable room_id + partial unique indexes.

ALTER TABLE chat_slow_modes ALTER COLUMN room_id DROP NOT NULL;

ALTER TABLE chat_slow_modes
    DROP CONSTRAINT IF EXISTS chat_slow_modes_room_id_target_user_id_key;

CREATE UNIQUE INDEX IF NOT EXISTS uq_chat_slow_modes_room_target
    ON chat_slow_modes (room_id, target_user_id)
    WHERE room_id IS NOT NULL;

CREATE UNIQUE INDEX IF NOT EXISTS uq_chat_slow_modes_server_target
    ON chat_slow_modes (target_user_id)
    WHERE room_id IS NULL;
