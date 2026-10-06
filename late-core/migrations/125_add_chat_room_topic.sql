-- Room "about" info: what a room is about (`topic`, the same concept IRC
-- projects as RPL_TOPIC) and its general rules. Both nullable, so every
-- existing room reads as "unset" and renders unchanged.
--
-- `created_by` records who opened a room. It is written only by the create
-- paths from now on and is never back-filled: NULL means "no recorded
-- creator" (system rooms, DMs, and every room predating this migration).
-- Current ownership is derived from it at read time, see
-- `ChatRoom::owner_id`, so a creator who leaves hands the room to the next
-- remaining member without any write.
ALTER TABLE chat_rooms
    ADD COLUMN topic      TEXT,
    ADD COLUMN rules      TEXT,
    ADD COLUMN created_by UUID REFERENCES users(id) ON DELETE SET NULL;
