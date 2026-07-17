-- Rooms demolition (phase 3, devdocs/FRD-LOBBY-CONSOLIDATION.md): the Rooms
-- directory is gone; the four surviving games run as fixed house tables with
-- permanent chat rooms seeded at startup (no game_rooms row). Drop the
-- user-created tables plus their chat and voice surfaces.
--
-- House-table chat rooms (kind='game', slugs poker/blackjack/maze/tron) and
-- private daily-match chats have no game_rooms row and are untouched.

-- Rooms-era voice channels target the game_rooms row directly
-- (target_kind='game_room'); target_id has no FK, so clean explicitly.
DELETE FROM voice_channels
WHERE target_kind = 'game_room';

-- Belt and braces: any chat_room-targeted voice channel on a table's chat
-- room (none are expected, but target_id has no FK to prove it).
DELETE FROM voice_channels v
USING game_rooms g
WHERE v.target_kind = 'chat_room'
  AND v.target_id = g.chat_room_id;

-- The tables' public chat rooms. Cascades chat messages, memberships, and
-- the game_rooms rows themselves (game_rooms.chat_room_id ON DELETE CASCADE).
DELETE FROM chat_rooms c
USING game_rooms g
WHERE g.chat_room_id = c.id;

DROP TABLE game_rooms;
