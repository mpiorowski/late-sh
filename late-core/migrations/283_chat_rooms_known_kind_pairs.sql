-- chat_rooms holds exactly the (kind, visibility) pairs the code reads
-- (`chat_room::RoomKind::from_columns`): only topic and game rooms come in
-- both visibilities, a DM is its two members', and every other kind is
-- public. A row outside the list would panic every session that loads it,
-- so the table refuses it instead.
--
-- Existing rows are checked, not rewritten: the only repair for a private
-- lounge, language or deadchannel room is making it public, which would
-- show its messages to everyone. If this migration fails, look at the rows
-- and decide by hand:
--   SELECT kind, visibility, count(*) FROM chat_rooms GROUP BY 1, 2;
--
-- The list replaces the two checks it implies: the kind list and the
-- DM-only rule for the 'dm' visibility. A new kind is added here alone.

ALTER TABLE chat_rooms DROP CONSTRAINT chat_rooms_kind_check;
ALTER TABLE chat_rooms DROP CONSTRAINT chat_rooms_visibility_kind_chk;
ALTER TABLE chat_rooms ADD CONSTRAINT chat_rooms_kind_visibility_known
    CHECK ((kind, visibility) IN (
        ('lounge', 'public'),
        ('language', 'public'),
        ('topic', 'public'),
        ('topic', 'private'),
        ('dm', 'dm'),
        ('game', 'public'),
        ('game', 'private'),
        ('deadchannel', 'public'),
        ('nightcap', 'public')
    ));
