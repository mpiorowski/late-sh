DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM chat_rooms WHERE slug = 'lounge') THEN
        RAISE EXCEPTION 'cannot rename #general to #lounge while #lounge exists';
    END IF;
END $$;

ALTER TABLE chat_rooms DROP CONSTRAINT chat_rooms_general_slug_chk;
ALTER TABLE chat_rooms DROP CONSTRAINT chat_rooms_kind_check;
DROP INDEX IF EXISTS uq_chat_rooms_general_slug;

UPDATE chat_rooms
SET kind = 'lounge',
    slug = 'lounge',
    visibility = 'public',
    auto_join = true,
    permanent = true,
    updated = current_timestamp
WHERE kind = 'general' AND slug = 'general';

ALTER TABLE chat_rooms ADD CONSTRAINT chat_rooms_kind_check
    CHECK (kind IN ('lounge', 'language', 'dm', 'topic', 'game'));

ALTER TABLE chat_rooms ADD CONSTRAINT chat_rooms_lounge_slug_chk
    CHECK ((kind <> 'lounge') OR (slug = 'lounge'));

CREATE UNIQUE INDEX uq_chat_rooms_lounge_slug
ON chat_rooms (slug)
WHERE kind = 'lounge';
