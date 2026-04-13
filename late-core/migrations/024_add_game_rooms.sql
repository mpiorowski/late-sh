-- Add 'game' kind to chat_rooms for table games (blackjack first, poker later).
-- Each game room is bound to exactly one game type via game_kind and is
-- addressed by a slug unique within that game type. Game rooms are opt-in:
-- users join when they walk into the Arcade, not on login.

ALTER TABLE chat_rooms DROP CONSTRAINT chat_rooms_kind_check;
ALTER TABLE chat_rooms ADD CONSTRAINT chat_rooms_kind_check
    CHECK (kind IN ('general', 'language', 'dm', 'topic', 'game'));

ALTER TABLE chat_rooms ADD COLUMN game_kind TEXT;

-- game rooms require game_kind + slug; non-game rooms must not set game_kind.
ALTER TABLE chat_rooms ADD CONSTRAINT chat_rooms_game_kind_chk
    CHECK (
        (kind <> 'game' AND game_kind IS NULL)
        OR (kind = 'game' AND game_kind IS NOT NULL AND slug IS NOT NULL)
    );

-- game rooms never auto-join. Enforces the "opt-in by walking into the
-- Arcade" invariant at the DB level so no future code path can spam every
-- user's unread counts with blackjack chatter.
ALTER TABLE chat_rooms ADD CONSTRAINT chat_rooms_game_no_auto_join_chk
    CHECK (kind <> 'game' OR auto_join = false);

-- Unique slug per game_kind. Lets us host many blackjack tables side by side
-- (bj-001, bj-002, ...) while keeping each one addressable.
CREATE UNIQUE INDEX uq_chat_rooms_game_slug
ON chat_rooms (game_kind, slug)
WHERE kind = 'game';
