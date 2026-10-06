-- Per-room read cursors for pinned cIRC rooms: slug -> the newest message
-- timestamp seen while the user was in the room, in epoch milliseconds on
-- cyberspace.online's clock (their roster reports last_message_at on the
-- same clock, so the comparison never crosses clocks). Powers the rail's
-- unread dot; pruned to the pinned list whenever that list is rewritten.
ALTER TABLE cyberspace_accounts
    ADD COLUMN circ_room_reads jsonb NOT NULL DEFAULT '{}'::jsonb;
