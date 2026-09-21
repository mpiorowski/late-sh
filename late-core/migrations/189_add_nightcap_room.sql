-- Add the 'nightcap' kind: the small bar out back of the Clubhouse
-- (late-ssh/src/app/clubhouse/nightcap). A dedicated kind so every room
-- listing excludes it by construction (browse lists only 'topic', IRC lists
-- lounge/language/topic, the Home rail skips the kind): the room is only
-- ever seen from its own screen, and only the seated can speak there.

ALTER TABLE chat_rooms DROP CONSTRAINT chat_rooms_kind_check;
ALTER TABLE chat_rooms ADD CONSTRAINT chat_rooms_kind_check
    CHECK (kind IN ('lounge', 'language', 'dm', 'topic', 'game', 'deadchannel', 'nightcap'));

-- Always auto-joined and permanent, like #lounge: every session already
-- holds the room, so a visit never writes a membership row. The stool, not
-- the membership, is what gates writing.
ALTER TABLE chat_rooms ADD CONSTRAINT chat_rooms_nightcap_chk
    CHECK (kind <> 'nightcap' OR (auto_join = true AND permanent = true AND slug IS NOT NULL));

CREATE UNIQUE INDEX uq_chat_rooms_nightcap_slug
ON chat_rooms (slug)
WHERE kind = 'nightcap';
