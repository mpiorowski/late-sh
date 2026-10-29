-- When a daily match started: the claim that took it from `open` to
-- `active`. `created` is when the challenge was posted and `updated` moves
-- with every move, so nothing else on the row keeps the start. The paper's
-- YESTERDAY line counts the matches started on the covered day by it
-- (`leaderboard::day_lobby_starts`).
--
-- NULL for an open or cancelled challenge, and for a match claimed before
-- this column whose chat room is gone. The backfill reads the match's chat
-- room, which the claim creates in its own transaction, so the room's
-- `created` is the claim's `current_timestamp`; rooms older than the chat
-- sweep's 30 days were already dropped and their matches stay NULL.
ALTER TABLE daily_matches
    ADD COLUMN started_at TIMESTAMPTZ;

UPDATE daily_matches
SET started_at = chat_rooms.created
FROM chat_rooms
WHERE chat_rooms.id = daily_matches.chat_room_id
  AND daily_matches.status IN ('active', 'finished');
